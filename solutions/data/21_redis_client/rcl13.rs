use std::collections::VecDeque;
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

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

struct Request {
    args: Vec<String>,
    reply: oneshot::Sender<io::Result<Value>>,
}

fn fail_all(waiting: &mut VecDeque<oneshot::Sender<io::Result<Value>>>, why: &str) {
    for reply in waiting.drain(..) {
        let _ = reply.send(Err(io::Error::new(io::ErrorKind::ConnectionAborted, why.to_string())));
    }
}

async fn run_actor(mut stream: TcpStream, mut rx: mpsc::Receiver<Request>, max_in_flight: usize) {
    let mut waiting: VecDeque<oneshot::Sender<io::Result<Value>>> = VecDeque::new();
    let mut buf = BytesMut::new();
    let mut open = true;
    while open || !waiting.is_empty() {
        tokio::select! {
            req = rx.recv(), if open && waiting.len() < max_in_flight => match req {
                Some(req) => {
                    if let Err(e) = stream.write_all(&encode_command(req.args.as_slice())).await {
                        let _ = req.reply.send(Err(e));
                        fail_all(&mut waiting, "соединение оборвалось при записи");
                        return;
                    }
                    waiting.push_back(req.reply);
                }
                None => open = false,
            },
            read = stream.read_buf(&mut buf) => {
                if !matches!(read, Ok(n) if n > 0) {
                    fail_all(&mut waiting, "соединение закрыто сервером");
                    return;
                }
                loop {
                    match decode(&mut buf) {
                        Ok(Some(value)) => {
                            if let Some(reply) = waiting.pop_front() {
                                let _ = reply.send(Ok(value));
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            fail_all(&mut waiting, "нарушение протокола");
                            return;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct Client {
    tx: mpsc::Sender<Request>,
    addr: String,
}

impl Client {
    async fn connect(addr: &str) -> io::Result<Client> {
        let stream = TcpStream::connect(addr).await?;
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(run_actor(stream, rx, 64));
        Ok(Client { tx, addr: addr.to_string() })
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

fn parse_blpop(reply: Value) -> io::Result<Option<(String, Vec<u8>)>> {
    match reply {
        Value::Null => Ok(None),
        Value::Array(items) => match <[Value; 2]>::try_from(items) {
            Ok([Value::Bulk(list), Value::Bulk(value)]) => Ok(Some((String::from_utf8_lossy(&list).into_owned(), value))),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "BLPOP вернул массив неожиданной формы")),
        },
        Value::Error(text) => Err(io::Error::other(text)),
        other => Err(io::Error::new(io::ErrorKind::InvalidData, format!("на BLPOP пришёл неожиданный ответ {other:?}"))),
    }
}

impl Client {
    async fn blpop(&self, key: &str, timeout_secs: u32) -> io::Result<Option<(String, Vec<u8>)>> {
        let seconds = timeout_secs.to_string();
        let dedicated = Client::connect(&self.addr).await?;
        parse_blpop(dedicated.call(&["BLPOP", key, seconds.as_str()]).await?)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{FakeServer, Scope, Step};
    use tokio::time::{sleep, timeout};

    use std::time::Duration;

    #[tokio::test]
    async fn waiting_for_a_task_does_not_block_other_calls() {
        let scope = Scope::new().await;
        let list = scope.key("jobs");
        let client = Client::connect(&scope.addr()).await.expect("подключение");
        let waiter = {
            let (client, list) = (client.clone(), list.clone());
            tokio::spawn(async move { client.blpop(&list, 3).await })
        };
        sleep(Duration::from_millis(150)).await;
        let pong = timeout(Duration::from_millis(600), client.call(&["PING"]))
            .await
            .expect("PING ждёт, пока кончится BLPOP: блокирующая команда держит общее соединение")
            .expect("PING");
        assert_eq!(pong, Value::Simple("PONG".into()), "обычный вызов во время BLPOP");
        client.call(&["RPUSH", list.as_str(), "job-1"]).await.expect("RPUSH");
        let got = timeout(Duration::from_secs(2), waiter).await.expect("BLPOP не проснулся после RPUSH").expect("задача").expect("BLPOP");
        assert_eq!(got, Some((list, b"job-1".to_vec())), "воркер получил задачу из списка");
    }

    #[tokio::test]
    async fn blpop_times_out_with_none() {
        let scope = Scope::new().await;
        let client = Client::connect(&scope.addr()).await.expect("подключение");
        let got = timeout(Duration::from_secs(4), client.blpop(&scope.key("empty"), 1)).await.expect("завис").expect("BLPOP");
        assert_eq!(got, None, "в пустом списке BLPOP по сроку возвращает отсутствие значения");
    }

    #[tokio::test]
    async fn dedicated_connection_is_closed_afterwards() {
        let server = FakeServer::start(|_, cmd| match cmd[0].as_str() {
            "BLPOP" => vec![Step::Sleep(Duration::from_millis(150)), Step::Write(b"*-1\r\n".to_vec())],
            _ => vec![Step::Write(b"+PONG\r\n".to_vec())],
        })
        .await;
        let client = Client::connect(&server.addr()).await.expect("подключение");
        let waiter = {
            let client = client.clone();
            tokio::spawn(async move { client.blpop("q", 1).await })
        };
        sleep(Duration::from_millis(50)).await;
        assert_eq!(server.connections(), 2, "ожидание BLPOP должно идти на отдельном соединении");
        client.call(&["PING"]).await.expect("PING на общем соединении");
        waiter.await.expect("задача").expect("BLPOP");
        for _ in 0..50 {
            if server.open_connections() == 1 {
                return;
            }
            sleep(Duration::from_millis(40)).await;
        }
        panic!("отдельное соединение после BLPOP осталось открытым: утечка по соединению на каждый вызов");
    }
}
