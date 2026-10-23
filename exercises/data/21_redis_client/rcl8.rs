// Чтобы клиент не висел на медленных командах, у него есть `call_timeout`. Он работает, пока серверу ничего
// не мешает. Но стоит команде один раз не уложиться в срок, как следующая, совсем другая команда
// получает чужой ответ: тот самый, на который отвели срок. Чем дальше, тем больше путаница: ответы
// сдвинуты на один, и каждый вызов видит ответ на предыдущий. Сделай так, чтобы отмена не портила соединение,
// где бы она ни случилась: через `call_timeout` или потому, что вызывающий просто бросил будущее.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::Duration;

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

    async fn call_timeout<S: AsRef<[u8]>>(&mut self, args: &[S], limit: Duration) -> io::Result<Value> {
        match tokio::time::timeout(limit, self.call(args)).await {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "команда не уложилась в срок")),
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{FakeServer, Step};
    use tokio::time::{sleep, timeout};

    fn pong_for_ping(cmd: &[String]) -> Option<Vec<Step>> {
        (cmd[0] == "PING").then(|| vec![Step::Write(b"+PONG\r\n".to_vec())])
    }

    #[tokio::test]
    async fn timeout_does_not_poison_the_next_call() {
        let server = FakeServer::start(|_, cmd| {
            pong_for_ping(&cmd).unwrap_or_else(|| vec![Step::Sleep(Duration::from_millis(300)), Step::Write(b"+late\r\n".to_vec())])
        })
        .await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        let err = conn.call_timeout(&["SLOW"], Duration::from_millis(50)).await.expect_err("команда медленнее срока");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "по сроку должна быть ошибка TimedOut");
        let got = timeout(Duration::from_secs(3), conn.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(got, Value::Simple("PONG".into()), "PING получил чужой ответ: поздний ответ на SLOW остался в сокете");
    }

    #[tokio::test]
    async fn timeout_in_the_middle_of_a_frame() {
        let server = FakeServer::start(|_, cmd| {
            pong_for_ping(&cmd).unwrap_or_else(|| {
                vec![Step::Write(b"$5\r\nhe".to_vec()), Step::Sleep(Duration::from_millis(250)), Step::Write(b"llo\r\n".to_vec())]
            })
        })
        .await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        conn.call_timeout(&["HALF"], Duration::from_millis(50)).await.expect_err("команда медленнее срока");
        let got = timeout(Duration::from_secs(3), conn.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(got, Value::Simple("PONG".into()), "ответ оборвали на полуслове, и в буфере остался обрывок, а потом придёт остаток: соединению больше нельзя верить");
    }

    #[tokio::test]
    async fn dropped_future_is_as_bad_as_a_timeout() {
        let server = FakeServer::start(|_, cmd| {
            pong_for_ping(&cmd).unwrap_or_else(|| vec![Step::Sleep(Duration::from_millis(300)), Step::Write(b"+late\r\n".to_vec())])
        })
        .await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        tokio::select! {
            _ = conn.call(&["SLOW"]) => panic!("SLOW не должна успеть"),
            _ = sleep(Duration::from_millis(50)) => {}
        }
        let got = timeout(Duration::from_secs(3), conn.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(got, Value::Simple("PONG".into()), "будущее бросили, не дождавшись ответа, и следующий вызов получил чужой: защита должна жить в `call`, а не только в `call_timeout`");
    }

    #[tokio::test]
    async fn healthy_connection_is_not_replaced() {
        let server = FakeServer::start(|_, cmd| pong_for_ping(&cmd).unwrap_or_default()).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        for _ in 0..20 {
            assert_eq!(conn.call_timeout(&["PING"], Duration::from_secs(1)).await.expect("PING"), Value::Simple("PONG".into()), "PING");
        }
        assert_eq!(server.connections(), 1, "пока всё идёт хорошо, переподключаться незачем: соединение одно, а их было {}", server.connections());
    }
}
