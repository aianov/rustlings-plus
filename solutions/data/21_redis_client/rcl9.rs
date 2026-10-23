use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
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
}

struct Request {
    args: Vec<String>,
    reply: oneshot::Sender<io::Result<Value>>,
}

#[derive(Clone)]
struct Client {
    tx: mpsc::Sender<Request>,
}

impl Client {
    async fn connect(addr: &str) -> io::Result<Client> {
        let mut conn = Conn::connect(addr).await?;
        let (tx, mut rx) = mpsc::channel::<Request>(64);
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let result = conn.call(req.args.as_slice()).await;
                let failed = result.is_err();
                let _ = req.reply.send(result);
                if failed {
                    break;
                }
            }
        });
        Ok(Client { tx })
    }

    async fn call(&self, args: &[&str]) -> io::Result<Value> {
        let (reply, answer) = oneshot::channel();
        let req = Request { args: args.iter().map(|s| s.to_string()).collect(), reply };
        if self.tx.send(req).await.is_err() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "соединение закрыто"));
        }
        match answer.await {
            Ok(result) => result,
            Err(_) => Err(io::Error::new(io::ErrorKind::BrokenPipe, "актор остановился")),
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{FakeServer, Scope, Step};
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    async fn fifty_tasks_share_one_connection() {
        let scope = Scope::new().await;
        let key = scope.key("counter");
        let client = Client::connect(&scope.addr()).await.expect("подключение");
        let mut tasks = Vec::new();
        for _ in 0..50 {
            let (client, key) = (client.clone(), key.clone());
            tasks.push(tokio::spawn(async move {
                let mut seen = Vec::new();
                for _ in 0..20 {
                    match client.call(&["INCR", key.as_str()]).await.expect("INCR") {
                        Value::Int(n) => seen.push(n),
                        other => panic!("INCR вернул {other:?}"),
                    }
                }
                seen
            }));
        }
        let mut all = Vec::new();
        for t in tasks {
            all.extend(timeout(Duration::from_secs(10), t).await.expect("завис").expect("задача"));
        }
        all.sort_unstable();
        assert_eq!(all, (1..=1000).collect::<Vec<i64>>(), "каждый INCR должен получить свой ответ, без дублей и пропусков");
    }

    #[tokio::test]
    async fn every_caller_gets_its_own_reply() {
        let scope = Scope::new().await;
        let client = Client::connect(&scope.addr()).await.expect("подключение");
        let mut tasks = Vec::new();
        for i in 0..30 {
            let client = client.clone();
            let key = scope.key(&format!("k{i}"));
            tasks.push(tokio::spawn(async move {
                client.call(&["SET", key.as_str(), i.to_string().as_str()]).await.expect("SET");
                let got = client.call(&["GET", key.as_str()]).await.expect("GET");
                (i, got)
            }));
        }
        for t in tasks {
            let (i, got) = timeout(Duration::from_secs(10), t).await.expect("завис").expect("задача");
            assert_eq!(got, Value::Bulk(i.to_string().into_bytes()), "задача {i} получила чужой ответ: ответы перепутались между вызывающими");
        }
    }

    #[tokio::test]
    async fn last_client_dropped_closes_the_connection() {
        let server = FakeServer::start(|_, _| vec![Step::Write(b"+PONG\r\n".to_vec())]).await;
        let client = Client::connect(&server.addr()).await.expect("подключение");
        let copy = client.clone();
        copy.call(&["PING"]).await.expect("PING");
        drop(client);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(server.open_connections(), 1, "пока жива копия клиента, соединение нужно");
        drop(copy);
        for _ in 0..50 {
            if server.open_connections() == 0 {
                return;
            }
            sleep(Duration::from_millis(40)).await;
        }
        panic!("все копии клиента брошены, а соединение осталось открытым: актор должен закончиться, когда закрылся канал");
    }

    #[tokio::test]
    async fn broken_connection_gives_errors_not_hangs() {
        let server = FakeServer::start(|_, _| vec![Step::Close]).await;
        let client = Client::connect(&server.addr()).await.expect("подключение");
        for round in 1..=2 {
            let res = timeout(Duration::from_secs(2), client.call(&["PING"])).await.unwrap_or_else(|_| panic!("вызов {round} завис, а должен вернуть ошибку"));
            assert!(res.is_err(), "сервер закрыл соединение, вызов {round} не может быть успешным");
        }
    }
}
