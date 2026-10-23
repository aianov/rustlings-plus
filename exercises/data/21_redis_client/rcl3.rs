// Соединение с `call` уже умеет возвращать значения RESP, но пользоваться им неудобно: каждый раз
// надо руками собирать команду и разбирать enum. Нужны типизированные методы. GET должен отличать
// отсутствующий ключ от пустого значения, SET проверять, что сервер ответил OK, а неожиданный вид
// ответа не должен превращаться в тихий мусор.
// TODO: Напиши `get` и `set`. Ошибку сервера возвращай как `io::Error` вида Other с текстом ответа,
// ответ неожиданного вида как InvalidData.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use bytes::Bytes;

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

struct Conn {
    stream: TcpStream,
    buf: BytesMut,
}

impl Conn {
    async fn connect(addr: &str) -> io::Result<Conn> {
        Ok(Conn { stream: TcpStream::connect(addr).await?, buf: BytesMut::new() })
    }

    async fn call<S: AsRef<[u8]>>(&mut self, args: &[S]) -> io::Result<Value> {
        self.stream.write_all(&encode_command(args)).await?;
        loop {
            match decode(&mut self.buf) {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => {}
                Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "ответ не похож на RESP")),
            }
            if self.stream.read_buf(&mut self.buf).await? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "сервер закрыл соединение"));
            }
        }
    }
}

impl Conn {
    async fn get(&mut self, key: &str) -> io::Result<Option<Bytes>> {
        todo!()
    }

    async fn set(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{FakeServer, Scope, Step};
    use std::time::Duration;
    use tokio::time::timeout;

    fn reply(text: &str) -> Vec<Step> {
        vec![Step::Write(text.as_bytes().to_vec())]
    }

    async fn fresh() -> (Scope, Conn) {
        let scope = Scope::new().await;
        let conn = Conn::connect(&scope.addr()).await.expect("подключение");
        (scope, conn)
    }

    #[tokio::test]
    async fn text_round_trip() {
        let (scope, mut conn) = fresh().await;
        let key = scope.key("name");
        timeout(Duration::from_secs(3), conn.set(&key, "Мария".as_bytes())).await.expect("завис").expect("SET");
        let got = timeout(Duration::from_secs(3), conn.get(&key)).await.expect("завис").expect("GET");
        assert_eq!(got.as_deref(), Some("Мария".as_bytes()), "значение вернулось таким же, каким записали");
    }

    #[tokio::test]
    async fn missing_and_empty_are_different() {
        let (scope, mut conn) = fresh().await;
        let key = scope.key("empty");
        assert_eq!(conn.get(&key).await.expect("GET"), None, "ключа нет: None");
        conn.set(&key, b"").await.expect("SET пустого значения");
        assert_eq!(conn.get(&key).await.expect("GET").as_deref(), Some(&b""[..]), "ключ есть, значение пустое: Some, а не None");
    }

    #[tokio::test]
    async fn binary_and_big_values() {
        let (scope, mut conn) = fresh().await;
        let raw = [0u8, 255, b'\r', b'\n', b'$', b'1', b'\r', b'\n', 7];
        conn.set(&scope.key("bin"), &raw).await.expect("SET");
        assert_eq!(conn.get(&scope.key("bin")).await.expect("GET").as_deref(), Some(&raw[..]), "двоичное значение с CRLF внутри");
        let big = vec![b'z'; 1_000_000];
        conn.set(&scope.key("big"), &big).await.expect("SET мегабайта");
        let got = timeout(Duration::from_secs(5), conn.get(&scope.key("big"))).await.expect("завис на большом значении").expect("GET");
        assert_eq!(got.map(|b| b.len()), Some(1_000_000), "мегабайт целиком");
    }

    #[tokio::test]
    async fn server_error_is_other_kind_with_text() {
        let (scope, mut conn) = fresh().await;
        let key = scope.key("list");
        conn.call(&["RPUSH", key.as_str(), "a"]).await.expect("RPUSH");
        let err = conn.get(&key).await.expect_err("GET по списку должен вернуть ошибку типа");
        assert_eq!(err.kind(), io::ErrorKind::Other, "ошибка сервера это Other");
        assert!(err.to_string().contains("WRONGTYPE"), "в тексте должен быть ответ сервера, а там: {err}");
    }

    #[tokio::test]
    async fn unexpected_reply_kind_is_invalid_data() {
        let server = FakeServer::start(|_, cmd| if cmd[0] == "GET" { reply(":5\r\n") } else { reply("+QUEUED\r\n") }).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        let err = conn.get("k").await.expect_err("число в ответ на GET не значение");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "неожиданный вид ответа на GET");
        let err = conn.set("k", b"v").await.expect_err("QUEUED в ответ на SET не OK");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "SET принимает только статус OK");
    }
}
