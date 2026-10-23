// Актор из прошлой задачи отправляет команду, ждёт ответ, и только потом берёт следующую. Двадцать
// задач выстраиваются в очередь, хотя Redis готов принимать команды без остановки. Ответы сервера и так
// приходят строго в порядке команд, так что ждать нечего: пиши команды по мере поступления, а ответы
// раздавай ответчикам в том же порядке (FIFO).
// TODO: Перепиши `run_actor`: он принимает запросы и читает ответы одновременно. Если вызывающий
// ушёл (его oneshot закрыт), ответ всё равно надо прочитать и выбросить. Одновременно в полёте не больше
// `max_in_flight` команд: не принимай новые, пока очередь ответчиков полна.

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

fn fail_all(waiting: &mut VecDeque<oneshot::Sender<io::Result<Value>>>, why: &str) {
    for reply in waiting.drain(..) {
        let _ = reply.send(Err(io::Error::new(io::ErrorKind::ConnectionAborted, why.to_string())));
    }
}

async fn run_actor(stream: TcpStream, mut rx: mpsc::Receiver<Request>, max_in_flight: usize) {
    let mut conn = Conn { stream, buf: BytesMut::new() };
    while let Some(req) = rx.recv().await {
        let result = conn.call(req.args.as_slice()).await;
        let failed = result.is_err();
        let _ = req.reply.send(result);
        if failed {
            break;
        }
    }
}

#[derive(Clone)]
struct Client {
    tx: mpsc::Sender<Request>,
}

impl Client {
    async fn connect(addr: &str, max_in_flight: usize) -> io::Result<Client> {
        let stream = TcpStream::connect(addr).await?;
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(run_actor(stream, rx, max_in_flight));
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{sleep, timeout};

    async fn server_that_answers_after(n: usize) -> FakeServer {
        let seen = Arc::new(AtomicUsize::new(0));
        FakeServer::start(move |_, _| {
            let k = seen.fetch_add(1, Ordering::SeqCst) + 1;
            if k < n { Vec::new() } else { (1..=n).flat_map(|i| vec![Step::Write(format!("+r{i}\r\n").into_bytes())]).collect() }
        })
        .await
    }

    #[tokio::test]
    async fn commands_do_not_wait_for_each_other() {
        let server = server_that_answers_after(3).await;
        let client = Client::connect(&server.addr(), 64).await.expect("подключение");
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let client = client.clone();
            tasks.push(tokio::spawn(async move { client.call(&["X"]).await }));
        }
        let mut got = Vec::new();
        for t in tasks {
            let res = timeout(Duration::from_secs(2), t).await.expect("актор ждёт ответа на первую команду и не отправляет остальные: сервер отвечает, только когда получит все три").expect("задача").expect("ответ");
            got.push(res);
        }
        got.sort_by_key(|v| format!("{v:?}"));
        assert_eq!(got, vec![Value::Simple("r1".into()), Value::Simple("r2".into()), Value::Simple("r3".into())], "каждому свой ответ");
    }

    #[tokio::test]
    async fn abandoned_caller_does_not_shift_replies() {
        let server = FakeServer::start(|_, cmd| match cmd[0].as_str() {
            "SLOW" => vec![Step::Sleep(Duration::from_millis(200)), Step::Write(b"+slow\r\n".to_vec())],
            _ => vec![Step::Write(b"+PONG\r\n".to_vec())],
        })
        .await;
        let client = Client::connect(&server.addr(), 64).await.expect("подключение");
        let gave_up = timeout(Duration::from_millis(50), client.call(&["SLOW"])).await;
        assert!(gave_up.is_err(), "SLOW не должна успеть за 50 мс");
        let got = timeout(Duration::from_secs(3), client.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(got, Value::Simple("PONG".into()), "ответ на брошенный запрос надо прочитать и выбросить, иначе PING получит его");
    }

    #[tokio::test]
    async fn in_flight_is_bounded() {
        let server = FakeServer::start(|_, _| Vec::new()).await;
        let client = Client::connect(&server.addr(), 3).await.expect("подключение");
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let client = client.clone();
            tasks.push(tokio::spawn(async move { timeout(Duration::from_millis(300), client.call(&["X"])).await.is_err() }));
        }
        for t in tasks {
            assert!(t.await.expect("задача"), "сервер молчит, каждый вызов должен кончиться по сроку");
        }
        sleep(Duration::from_millis(100)).await;
        assert_eq!(server.commands().len(), 3, "до сервера должны дойти ровно три команды (предел 3), а дошло {}", server.commands().len());
    }

    #[tokio::test]
    async fn hundred_tasks_on_real_redis() {
        let scope = Scope::new().await;
        let key = scope.key("n");
        let client = Client::connect(&scope.addr(), 32).await.expect("подключение");
        let mut tasks = Vec::new();
        for _ in 0..100 {
            let (client, key) = (client.clone(), key.clone());
            tasks.push(tokio::spawn(async move { client.call(&["INCR", key.as_str()]).await }));
        }
        let mut seen = Vec::new();
        for t in tasks {
            match timeout(Duration::from_secs(5), t).await.expect("завис").expect("задача").expect("INCR") {
                Value::Int(n) => seen.push(n),
                other => panic!("INCR вернул {other:?}"),
            }
        }
        seen.sort_unstable();
        assert_eq!(seen, (1..=100).collect::<Vec<i64>>(), "все сто ответов разные и на своих местах");
    }
}
