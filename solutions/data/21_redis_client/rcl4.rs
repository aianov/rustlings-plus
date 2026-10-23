use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::fmt;

use bytes::{Buf, BytesMut};

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Null,
    Array(Vec<Value>),
}

#[derive(Debug)]
struct ProtoError;

fn encode_command<S: AsRef<[u8]>>(args: &[S]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for arg in args {
        let arg = arg.as_ref();
        out.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn parse_at(buf: &[u8], pos: usize) -> Result<Option<(Value, usize)>, ProtoError> {
    let Some(rel) = buf.get(pos..).and_then(|rest| rest.windows(2).position(|w| w == b"\r\n")) else {
        return Ok(None);
    };
    let end = pos + rel;
    let after = end + 2;
    let line = std::str::from_utf8(buf.get(pos + 1..end).ok_or(ProtoError)?).map_err(|_| ProtoError)?;
    match buf[pos] {
        b'+' => Ok(Some((Value::Simple(line.to_string()), after))),
        b'-' => Ok(Some((Value::Error(line.to_string()), after))),
        b':' => Ok(Some((Value::Int(line.parse().map_err(|_| ProtoError)?), after))),
        b'$' => {
            let len: i64 = line.parse().map_err(|_| ProtoError)?;
            if len < 0 {
                return Ok(Some((Value::Null, after)));
            }
            let len = len as usize;
            if buf.len() < after + len + 2 {
                return Ok(None);
            }
            Ok(Some((Value::Bulk(buf[after..after + len].to_vec()), after + len + 2)))
        }
        b'*' => {
            let n: i64 = line.parse().map_err(|_| ProtoError)?;
            if n < 0 {
                return Ok(Some((Value::Null, after)));
            }
            let mut items = Vec::new();
            let mut next = after;
            for _ in 0..n {
                let Some((item, item_end)) = parse_at(buf, next)? else {
                    return Ok(None);
                };
                items.push(item);
                next = item_end;
            }
            Ok(Some((Value::Array(items), next)))
        }
        _ => Err(ProtoError),
    }
}

fn decode(src: &mut BytesMut) -> Result<Option<Value>, ProtoError> {
    match parse_at(src, 0)? {
        Some((value, used)) => {
            src.advance(used);
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

#[derive(Debug)]
enum Error {
    Io(io::Error),
    Protocol(ProtoError),
    Server { kind: String, message: String },
    Unexpected(Value),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<ProtoError> for Error {
    fn from(e: ProtoError) -> Error {
        Error::Protocol(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "ошибка ввода-вывода: {e}"),
            Error::Protocol(_) => write!(f, "сервер нарушил протокол"),
            Error::Server { kind, message } => write!(f, "{kind}: {message}"),
            Error::Unexpected(value) => write!(f, "неожиданный ответ: {value:?}"),
        }
    }
}

impl Error {
    fn from_reply(text: &str) -> Error {
        let (kind, message) = text.split_once(' ').unwrap_or((text, ""));
        Error::Server { kind: kind.to_string(), message: message.trim_start().to_string() }
    }

    fn is_retryable(&self) -> bool {
        match self {
            Error::Io(_) => true,
            Error::Server { kind, .. } => matches!(kind.as_str(), "LOADING" | "BUSY" | "TRYAGAIN" | "CLUSTERDOWN" | "MASTERDOWN"),
            Error::Protocol(_) | Error::Unexpected(_) => false,
        }
    }
}

struct Conn {
    stream: TcpStream,
    buf: BytesMut,
}

impl Conn {
    async fn connect(addr: &str) -> Result<Conn, Error> {
        Ok(Conn { stream: TcpStream::connect(addr).await?, buf: BytesMut::new() })
    }

    async fn call<S: AsRef<[u8]>>(&mut self, args: &[S]) -> Result<Value, Error> {
        self.stream.write_all(&encode_command(args)).await?;
        loop {
            match decode(&mut self.buf)? {
                Some(Value::Error(text)) => return Err(Error::from_reply(&text)),
                Some(value) => return Ok(value),
                None => {}
            }
            if self.stream.read_buf(&mut self.buf).await? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "сервер закрыл соединение").into());
            }
        }
    }

    async fn incr(&mut self, key: &str) -> Result<i64, Error> {
        match self.call(&["INCR", key]).await? {
            Value::Int(n) => Ok(n),
            other => Err(Error::Unexpected(other)),
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{FakeServer, Step};
    use std::time::Duration;
    use tokio::time::timeout;

    fn reply(text: &str) -> Vec<Step> {
        vec![Step::Write(text.as_bytes().to_vec())]
    }

    #[test]
    fn reply_is_split_into_kind_and_message() {
        match Error::from_reply("WRONGTYPE Operation against a key holding the wrong kind of value") {
            Error::Server { kind, message } => {
                assert_eq!(kind, "WRONGTYPE", "вид ошибки это первое слово");
                assert_eq!(message, "Operation against a key holding the wrong kind of value", "остальное это пояснение целиком");
            }
            other => panic!("ответ-ошибка сервера должен стать Error::Server, а получено {other:?}"),
        }
        match Error::from_reply("ERR") {
            Error::Server { kind, message } => {
                assert_eq!(kind, "ERR", "вид без пояснения");
                assert_eq!(message, "", "пояснения нет");
            }
            other => panic!("ожидался Error::Server, а получено {other:?}"),
        }
    }

    #[test]
    fn retry_decision() {
        let server = |kind: &str| Error::from_reply(&format!("{kind} something"));
        for kind in ["LOADING", "BUSY", "TRYAGAIN"] {
            assert!(server(kind).is_retryable(), "{kind} пройдёт позже без правки команды");
        }
        for kind in ["WRONGTYPE", "ERR", "NOAUTH", "NOPERM", "OOM"] {
            assert!(!server(kind).is_retryable(), "{kind} от повтора не исчезнет");
        }
        assert!(Error::Io(io::Error::from(io::ErrorKind::ConnectionReset)).is_retryable(), "оборванное соединение: можно переподключиться и повторить");
        assert!(!Error::Unexpected(Value::Int(1)).is_retryable(), "ответ не того вида: так повторять бессмысленно");
        assert!(!Error::Protocol(ProtoError).is_retryable(), "нарушение протокола: повтор на том же соединении невозможен");
    }

    #[test]
    fn display_shows_kind_and_message() {
        let text = Error::from_reply("NOAUTH Authentication required.").to_string();
        assert!(text.contains("NOAUTH") && text.contains("Authentication required."), "в сообщении должны быть и вид, и пояснение, а там: {text}");
        let text = Error::Unexpected(Value::Simple("QUEUED".into())).to_string();
        assert!(text.contains("QUEUED"), "в сообщении о неожиданном ответе виден сам ответ: {text}");
        let text = Error::Io(io::Error::other("обрыв")).to_string();
        assert!(text.contains("обрыв"), "причина ввода-вывода не потеряна: {text}");
    }

    #[tokio::test]
    async fn errors_through_a_connection() {
        let server = FakeServer::start(|_, cmd| match cmd[1].as_str() {
            "loading" => reply("-LOADING Redis is loading\r\n"),
            "text" => reply("+OK\r\n"),
            "bad" => reply("!junk\r\n"),
            _ => vec![Step::Close],
        })
        .await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        assert!(matches!(timeout(Duration::from_secs(3), conn.incr("loading")).await.expect("завис"), Err(Error::Server { ref kind, .. }) if kind == "LOADING"), "ошибка сервера через соединение");
        assert!(matches!(timeout(Duration::from_secs(3), conn.incr("text")).await.expect("завис"), Err(Error::Unexpected(Value::Simple(_)))), "статус в ответ на INCR это Unexpected");
        assert!(matches!(timeout(Duration::from_secs(3), conn.incr("bad")).await.expect("завис"), Err(Error::Protocol(_))), "мусор вместо RESP");
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        let err = timeout(Duration::from_secs(3), conn.incr("closed")).await.expect("завис").expect_err("сервер закрыл соединение");
        assert!(matches!(err, Error::Io(_)) && err.is_retryable(), "закрытие соединения это повторяемая ошибка ввода-вывода");
    }
}
