use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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

async fn ping(addr: &str, message: Option<&str>) -> io::Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    let command: Vec<&str> = match message {
        Some(text) => vec!["PING", text],
        None => vec!["PING"],
    };
    stream.write_all(&encode_command(&command)).await?;
    let mut buf = BytesMut::with_capacity(512);
    stream.read_buf(&mut buf).await?;
    match decode(&mut buf) {
        Ok(Some(Value::Simple(text))) => Ok(text),
        Ok(Some(Value::Bulk(bytes))) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Ok(Some(Value::Error(text))) => Err(io::Error::other(text)),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "ответ на PING не распознан")),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{self, FakeServer, Step};
    use std::time::Duration;
    use tokio::time::timeout;

    fn reply(text: &str) -> Vec<Step> {
        vec![Step::Write(text.as_bytes().to_vec())]
    }

    #[tokio::test]
    async fn plain_ping_answers_pong() {
        let got = timeout(Duration::from_secs(5), ping(&redis::addr(), None)).await.expect("Redis не ответил за 5 секунд").expect("PING");
        assert_eq!(got, "PONG", "PING без сообщения отвечает статусом PONG");
    }

    #[tokio::test]
    async fn ping_with_message_returns_it() {
        let got = timeout(Duration::from_secs(5), ping(&redis::addr(), Some("привет мир"))).await.expect("ответ не пришёл").expect("PING с сообщением");
        assert_eq!(got, "привет мир", "с сообщением ответ приходит bulk-строкой, а не статусом; пробел и русские буквы доезжают целыми");
    }

    #[tokio::test]
    async fn message_travels_as_one_argument() {
        let server = FakeServer::start(|_, cmd| {
            let echo = cmd.get(1).cloned().unwrap_or_default();
            reply(&format!("${}\r\n{echo}\r\n", echo.len()))
        })
        .await;
        let got = timeout(Duration::from_secs(5), ping(&server.addr(), Some("два слова"))).await.expect("ответ не пришёл").expect("PING");
        assert_eq!(got, "два слова", "ответ сервера");
        assert_eq!(server.commands(), vec![vec!["PING".to_string(), "два слова".to_string()]], "сервер должен получить ровно два аргумента: PING и сообщение целиком");
    }

    #[tokio::test]
    async fn server_error_becomes_io_error() {
        let server = FakeServer::start(|_, _| reply("-LOADING Redis is loading the dataset in memory\r\n")).await;
        let err = timeout(Duration::from_secs(5), ping(&server.addr(), None)).await.expect("ответ не пришёл").expect_err("ошибку сервера нельзя выдавать за успех");
        assert!(err.to_string().contains("LOADING"), "в тексте ошибки должен быть ответ сервера, а там: {err}");
    }
}
