// В клиенте нет ни одного срока. Сервер принял соединение и замолчал (завис, не успел загрузиться, потерялся
// пакет), и приложение ждёт его вечно: подключение вечно, каждая команда вечно. Нужны два срока.
// Первый на подключение: TCP может открыться, а Redis так и не ответить, поэтому подключением считай
// момент, когда сервер ответил на PING. Второй на каждую команду. Актор из прошлых задач устроен так, что
// отмена вызова ему не вредит, так что сроку не нужны никакие хитрости с соединением.
// TODO: Допиши `connect` (с `PING` и сроком) и `call` (со сроком). Срок это ошибка вида TimedOut.

use std::collections::VecDeque;
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

#[derive(Clone, Copy)]
struct Options {
    connect_timeout: Duration,
    command_timeout: Duration,
}

#[derive(Clone)]
struct Client {
    tx: mpsc::Sender<Request>,
    options: Options,
}

fn start(stream: TcpStream, options: Options) -> Client {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(run_actor(stream, rx, 64));
    Client { tx, options }
}

impl Client {
    async fn connect(addr: &str, options: Options) -> io::Result<Client> {
        let stream = TcpStream::connect(addr).await?;
        Ok(start(stream, options))
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
    use course_support::redis::{FakeServer, Step};
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    fn options(connect_ms: u64, command_ms: u64) -> Options {
        Options { connect_timeout: Duration::from_millis(connect_ms), command_timeout: Duration::from_millis(command_ms) }
    }

    fn handler(cmd: &[String]) -> Vec<Step> {
        match cmd[0].as_str() {
            "SLOW" => vec![Step::Sleep(Duration::from_millis(300)), Step::Write(b"+late\r\n".to_vec())],
            _ => vec![Step::Write(b"+PONG\r\n".to_vec())],
        }
    }

    #[tokio::test]
    async fn silent_server_fails_the_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
        let addr = listener.local_addr().expect("адрес").to_string();
        let held = tokio::spawn(async move {
            let mut keep = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                keep.push(stream);
            }
        });
        let res = timeout(Duration::from_secs(2), Client::connect(&addr, options(200, 1000))).await.expect("подключение к молчащему серверу не уложилось в собственный срок 200 мс");
        let err = res.err().expect("сервер так и не ответил на PING, подключение не удалось");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "срок подключения это TimedOut");
        held.abort();
    }

    #[tokio::test]
    async fn slow_command_times_out_without_breaking_the_connection() {
        let server = FakeServer::start(|_, cmd| handler(&cmd)).await;
        let client = Client::connect(&server.addr(), options(1000, 100)).await.expect("подключение");
        let err = client.call(&["SLOW"]).await.expect_err("команда медленнее срока");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "срок команды это TimedOut");
        tokio::time::sleep(Duration::from_millis(400)).await;
        let pong = timeout(Duration::from_secs(2), client.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(pong, Value::Simple("PONG".into()), "после срока следующая команда получает свой ответ, а не запоздавший");
        assert_eq!(server.connections(), 1, "актор переживает срок без переподключения: соединение одно");
    }

    #[tokio::test]
    async fn healthy_calls_are_not_cut() {
        let server = FakeServer::start(|_, cmd| handler(&cmd)).await;
        let client = Client::connect(&server.addr(), options(1000, 1000)).await.expect("подключение");
        for _ in 0..10 {
            assert_eq!(client.call(&["PING"]).await.expect("PING"), Value::Simple("PONG".into()), "быстрые команды укладываются в срок");
        }
    }
}
