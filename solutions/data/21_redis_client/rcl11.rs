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

enum Outcome {
    Closed,
    Broken,
}

async fn serve(mut stream: TcpStream, rx: &mut mpsc::Receiver<Request>) -> Outcome {
    let mut waiting: VecDeque<oneshot::Sender<io::Result<Value>>> = VecDeque::new();
    let mut buf = BytesMut::new();
    let mut open = true;
    while open || !waiting.is_empty() {
        tokio::select! {
            req = rx.recv(), if open => match req {
                Some(req) => {
                    if let Err(e) = stream.write_all(&encode_command(req.args.as_slice())).await {
                        let _ = req.reply.send(Err(e));
                        fail_all(&mut waiting, "соединение оборвалось при записи");
                        return Outcome::Broken;
                    }
                    waiting.push_back(req.reply);
                }
                None => open = false,
            },
            read = stream.read_buf(&mut buf) => {
                if !matches!(read, Ok(n) if n > 0) {
                    fail_all(&mut waiting, "соединение закрыто сервером");
                    return Outcome::Broken;
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
                            return Outcome::Broken;
                        }
                    }
                }
            }
        }
    }
    Outcome::Closed
}

#[derive(Clone)]
struct Client {
    tx: mpsc::Sender<Request>,
}

impl Client {
    async fn connect(addr: &str, pause: Duration) -> io::Result<Client> {
        let first = TcpStream::connect(addr).await?;
        let (tx, mut rx) = mpsc::channel::<Request>(64);
        let addr = addr.to_string();
        tokio::spawn(async move {
            let mut stream = first;
            while let Outcome::Broken = serve(stream, &mut rx).await {
                stream = loop {
                    if rx.is_closed() {
                        return;
                    }
                    tokio::time::sleep(pause).await;
                    if let Ok(fresh) = TcpStream::connect(&addr).await {
                        break fresh;
                    }
                };
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
    use course_support::redis::{FakeServer, Step};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout};

    fn pong_server_handler(cmd: &[String]) -> Vec<Step> {
        match cmd[0].as_str() {
            "DIE" => vec![Step::Close],
            "HANG" => Vec::new(),
            _ => vec![Step::Write(b"+PONG\r\n".to_vec())],
        }
    }

    #[tokio::test]
    async fn pending_requests_fail_and_the_client_comes_back() {
        let server = FakeServer::start(|_, cmd| pong_server_handler(&cmd)).await;
        let client = Client::connect(&server.addr(), Duration::from_millis(50)).await.expect("подключение");
        let hanging = {
            let client = client.clone();
            tokio::spawn(async move { client.call(&["HANG"]).await })
        };
        sleep(Duration::from_millis(50)).await;
        let died = timeout(Duration::from_secs(2), client.call(&["DIE"])).await.expect("DIE завис");
        assert!(died.is_err(), "соединение оборвано, DIE не может быть успешной");
        let hung = timeout(Duration::from_secs(2), hanging).await.expect("запрос, ушедший в оборванное соединение, завис: ему надо вернуть ошибку при обрыве").expect("задача");
        assert!(hung.is_err(), "ответа на HANG не будет, нужна ошибка");
        let pong = timeout(Duration::from_secs(3), client.call(&["PING"])).await.expect("после обрыва клиент так и не вернулся").expect("PING");
        assert_eq!(pong, Value::Simple("PONG".into()), "после переподключения клиент работает");
        assert_eq!(server.connections(), 2, "второе соединение, не больше");
    }

    #[tokio::test]
    async fn request_waits_for_reconnect_instead_of_failing() {
        let server = FakeServer::start(|_, cmd| pong_server_handler(&cmd)).await;
        let client = Client::connect(&server.addr(), Duration::from_millis(200)).await.expect("подключение");
        assert!(client.call(&["DIE"]).await.is_err(), "обрыв");
        let pong = timeout(Duration::from_secs(3), client.call(&["PING"])).await.expect("завис").expect("запрос в паузу переподключения должен дождаться нового соединения, а не упасть");
        assert_eq!(pong, Value::Simple("PONG".into()), "ответ с нового соединения");
    }

    #[tokio::test]
    async fn flapping_server_is_survived() {
        let server = FakeServer::start(|conn, cmd| if conn < 2 { vec![Step::Close] } else { pong_server_handler(&cmd) }).await;
        let client = Client::connect(&server.addr(), Duration::from_millis(30)).await.expect("подключение");
        let mut ok = false;
        for _ in 0..6 {
            if let Ok(Ok(Value::Simple(_))) = timeout(Duration::from_secs(2), client.call(&["PING"])).await {
                ok = true;
                break;
            }
        }
        assert!(ok, "два первых соединения оборвали, третье работает: за шесть попыток клиент обязан дойти до него");
    }

    #[tokio::test]
    async fn reconnect_is_paced() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
        let addr = listener.local_addr().expect("адрес").to_string();
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = accepted.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        let _client = Client::connect(&addr, Duration::from_millis(100)).await.expect("подключение");
        sleep(Duration::from_millis(650)).await;
        let n = accepted.load(Ordering::SeqCst);
        assert!(n >= 3, "сервер рвёт каждое соединение, а клиент должен пытаться подключиться снова (за 650 мс при паузе 100 мс ожидается около 6 соединений, а было {n})");
        assert!(n <= 10, "клиент долбится в сервер без пауз: за 650 мс при паузе 100 мс было {n} соединений");
    }
}
