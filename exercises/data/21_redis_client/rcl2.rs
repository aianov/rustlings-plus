// Клиент читает ответ сервера одним `read` и разбирает то, что пришло. Пока ответы короткие, всё
// работает. Но большой ответ приходит несколькими кусками, а два ответа подряд иногда слипаются в один
// кусок, и тогда клиент либо жалуется на «неполный ответ», либо отдаёт чужой ответ на следующую команду.
// TODO: Сделай `call` надёжным: он должен возвращать ровно один ответ на команду, как бы ни были
// нарезаны байты.

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
        let mut chunk = [0u8; 1024];
        let n = self.stream.read(&mut chunk).await?;
        self.buf.extend_from_slice(&chunk[..n]);
        match decode(&mut self.buf) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Err(io::Error::other("ответ пришёл не целиком")),
            Err(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "ответ не похож на RESP")),
        }
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

    async fn call(conn: &mut Conn, args: &[&str]) -> Value {
        timeout(Duration::from_secs(3), conn.call(args))
            .await
            .expect("клиент завис: ответ уже пришёл, а он его не заметил или ждёт лишнего")
            .expect("ошибка ввода-вывода")
    }

    #[tokio::test]
    async fn reply_split_into_pieces() {
        let server = FakeServer::start(|_, _| {
            vec![Step::Write(b"$5\r\nhe".to_vec()), Step::Sleep(Duration::from_millis(60)), Step::Write(b"llo\r\n".to_vec())]
        })
        .await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        assert_eq!(call(&mut conn, &["GET", "k"]).await, Value::Bulk(b"hello".to_vec()), "ответ пришёл двумя кусками с паузой, и клиент должен дочитать");
    }

    #[tokio::test]
    async fn two_replies_glued_together() {
        let server = FakeServer::start(|_, cmd| if cmd[0] == "A" { reply("+one\r\n+two\r\n") } else { Vec::new() }).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        assert_eq!(call(&mut conn, &["A"]).await, Value::Simple("one".into()), "первый ответ");
        assert_eq!(call(&mut conn, &["B"]).await, Value::Simple("two".into()), "второй ответ уже лежал в буфере после первого чтения: сервер на B ничего не пришлёт, ответ надо взять из буфера, а не читать сокет");
    }

    #[tokio::test]
    async fn big_reply() {
        let mut wire = b"$300000\r\n".to_vec();
        wire.extend(vec![b'x'; 300_000]);
        wire.extend_from_slice(b"\r\n");
        let server = FakeServer::start(move |_, _| vec![Step::Write(wire.clone())]).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        match call(&mut conn, &["GET", "big"]).await {
            Value::Bulk(b) => assert_eq!(b.len(), 300_000, "значение пришло не целиком"),
            other => panic!("ожидалась bulk-строка, а пришло {other:?}"),
        }
    }

    #[tokio::test]
    async fn connection_closed_mid_reply() {
        let server = FakeServer::start(|_, _| vec![Step::Write(b"$5\r\nhe".to_vec()), Step::Close]).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        let err = timeout(Duration::from_secs(3), conn.call(&["GET", "k"])).await.expect("клиент завис вместо ошибки").expect_err("ответ оборван");
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof, "сервер закрыл соединение на полуслове: это UnexpectedEof, а не молчание");
    }

    #[tokio::test]
    async fn real_redis_round_trip() {
        let scope = redis::Scope::new().await;
        let key = scope.key("v");
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        assert_eq!(call(&mut conn, &["SET", key.as_str(), "x"]).await, Value::Simple("OK".into()), "SET");
        assert_eq!(call(&mut conn, &["GET", key.as_str()]).await, Value::Bulk(b"x".to_vec()), "GET");
    }
}
