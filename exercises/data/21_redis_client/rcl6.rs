// Тысяча команд по одной это тысяча обменов «записали, ждём ответа». Каждый стоит полного круга по сети,
// а сам Redis при этом скучает. Если записать команды все сразу и только потом прочитать ответы,
// круг останется один. Это pipelining, и сервер к нему готов: ответы вернутся в порядке команд.
// TODO: Напиши `pipeline`: все команды уходят вместе, ответов приходит столько же, и они в том же порядке.
// Ошибка одной команды не должна ломать остальные ответы пачки.

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
    async fn pipeline(&mut self, cmds: &[Vec<String>]) -> io::Result<Vec<Value>> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{FakeServer, Scope, Step};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::timeout;

    fn reply(text: &str) -> Vec<Step> {
        vec![Step::Write(text.as_bytes().to_vec())]
    }

    fn cmd(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn replies_come_in_command_order() {
        let scope = Scope::new().await;
        let (a, b) = (scope.key("a"), scope.key("b"));
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        let got = timeout(
            Duration::from_secs(5),
            conn.pipeline(&[cmd(&["SET", &a, "1"]), cmd(&["INCR", &a]), cmd(&["SET", &b, "x"]), cmd(&["GET", &a]), cmd(&["GET", &b])]),
        )
        .await
        .expect("завис")
        .expect("pipeline");
        assert_eq!(
            got,
            vec![Value::Simple("OK".into()), Value::Int(2), Value::Simple("OK".into()), Value::Bulk(b"2".to_vec()), Value::Bulk(b"x".to_vec())],
            "ответы должны идти в порядке команд"
        );
    }

    #[tokio::test]
    async fn one_error_does_not_break_the_rest() {
        let scope = Scope::new().await;
        let k = scope.key("k");
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        let got = conn.pipeline(&[cmd(&["SET", &k, "text"]), cmd(&["INCR", &k]), cmd(&["GET", &k])]).await.expect("ошибка команды внутри пачки не ошибка pipeline");
        assert!(matches!(&got[1], Value::Error(text) if text.contains("not an integer")), "второй ответ должен быть ошибкой сервера, а там {:?}", got[1]);
        assert_eq!(got[2], Value::Bulk(b"text".to_vec()), "третий ответ на месте");
        let pong = timeout(Duration::from_secs(3), conn.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(pong, Value::Simple("PONG".into()), "следующая команда получает свой ответ: если после ошибки остались непрочитанные ответы, соединение рассинхронизировано");
    }

    #[tokio::test]
    async fn all_commands_go_out_before_the_first_reply_is_awaited() {
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let server = FakeServer::start(move |_, _| {
            if counter.fetch_add(1, Ordering::SeqCst) + 1 < 3 { Vec::new() } else { reply("+r1\r\n+r2\r\n+r3\r\n") }
        })
        .await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        let got = timeout(Duration::from_secs(2), conn.pipeline(&[cmd(&["A"]), cmd(&["B"]), cmd(&["C"])]))
            .await
            .expect("клиент ждёт ответа на первую команду и не посылает остальные: сервер отвечает только когда получит все три")
            .expect("pipeline");
        assert_eq!(got.len(), 3, "три ответа на три команды");
    }

    #[tokio::test]
    async fn empty_batch_does_nothing() {
        let server = FakeServer::start(|_, _| Vec::new()).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        let got = timeout(Duration::from_secs(1), conn.pipeline(&[])).await.expect("пустая пачка не должна ждать ответов").expect("pipeline");
        assert!(got.is_empty(), "ответов нет");
    }

    #[tokio::test]
    async fn five_thousand_commands() {
        let scope = Scope::new().await;
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        let cmds: Vec<Vec<String>> = (0..5000).map(|i| vec!["SET".to_string(), scope.key(&format!("k{i}")), i.to_string()]).collect();
        let got = timeout(Duration::from_secs(8), conn.pipeline(&cmds)).await.expect("завис").expect("pipeline");
        assert_eq!(got.len(), 5000, "по ответу на каждую команду");
        assert!(got.iter().all(|v| *v == Value::Simple("OK".into())), "все SET успешны");
        assert_eq!(conn.call(&["GET", scope.key("k4999").as_str()]).await.expect("GET"), Value::Bulk(b"4999".to_vec()), "последняя запись на месте");
    }
}
