use std::io;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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

struct Pool {
    addr: String,
    idle: Arc<Mutex<Vec<Conn>>>,
    permits: Arc<Semaphore>,
}

struct Pooled {
    conn: Option<Conn>,
    idle: Arc<Mutex<Vec<Conn>>>,
    broken: bool,
    _permit: OwnedSemaphorePermit,
}

impl Deref for Pooled {
    type Target = Conn;

    fn deref(&self) -> &Conn {
        self.conn.as_ref().expect("соединение на месте до конца жизни охранника")
    }
}

impl DerefMut for Pooled {
    fn deref_mut(&mut self) -> &mut Conn {
        self.conn.as_mut().expect("соединение на месте до конца жизни охранника")
    }
}

impl Pool {
    fn new(addr: &str, size: usize) -> Pool {
        Pool { addr: addr.to_string(), idle: Arc::new(Mutex::new(Vec::new())), permits: Arc::new(Semaphore::new(size)) }
    }

    async fn get(&self) -> io::Result<Pooled> {
        let permit = self.permits.clone().acquire_owned().await.map_err(|_| io::Error::other("пул закрыт"))?;
        let reused = self.idle.lock().expect("список свободных").pop();
        let conn = match reused {
            Some(conn) => conn,
            None => Conn::connect(&self.addr).await?,
        };
        Ok(Pooled { conn: Some(conn), idle: self.idle.clone(), broken: false, _permit: permit })
    }
}

impl Pooled {
    fn discard(mut self) {
        self.broken = true;
    }
}

impl Drop for Pooled {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            if !self.broken {
                self.idle.lock().expect("список свободных").push(conn);
            }
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

    use std::time::Duration;

    #[tokio::test]
    async fn pool_never_exceeds_its_size() {
        let server = FakeServer::start(|_, _| vec![Step::Write(b"+PONG\r\n".to_vec())]).await;
        let pool = Pool::new(&server.addr(), 2);
        let first = pool.get().await.expect("первое");
        let second = pool.get().await.expect("второе");
        assert!(timeout(Duration::from_millis(150), pool.get()).await.is_err(), "лимит 2, третьему охраннику надо ждать, а он получил соединение");
        drop(first);
        let third = timeout(Duration::from_secs(2), pool.get()).await.expect("третий так и не дождался освободившегося соединения").expect("третье");
        drop((second, third));
        assert_eq!(server.connections(), 2, "третий должен переиспользовать соединение первого, а не открывать новое");
    }

    #[tokio::test]
    async fn connections_are_reused() {
        let server = FakeServer::start(|_, _| vec![Step::Write(b"+PONG\r\n".to_vec())]).await;
        let pool = Pool::new(&server.addr(), 3);
        for _ in 0..50 {
            let mut conn = pool.get().await.expect("get");
            assert_eq!(conn.call(&["PING"]).await.expect("PING"), Value::Simple("PONG".into()), "PING");
        }
        assert_eq!(server.connections(), 1, "пятьдесят последовательных использований обходятся одним соединением");
    }

    #[tokio::test]
    async fn broken_connection_is_not_returned_but_the_slot_is() {
        let server = FakeServer::start(|_, _| vec![Step::Write(b"+PONG\r\n".to_vec())]).await;
        let pool = Pool::new(&server.addr(), 1);
        let conn = pool.get().await.expect("get");
        conn.discard();
        let mut next = timeout(Duration::from_secs(2), pool.get()).await.expect("место в пуле не освободилось после discard: лимит 1, и новое соединение не получить").expect("get");
        assert_eq!(next.call(&["PING"]).await.expect("PING"), Value::Simple("PONG".into()), "новое соединение работает");
        assert_eq!(server.connections(), 2, "испорченное соединение не переиспользуется, открыто новое");
    }

    #[tokio::test]
    async fn multi_exec_blocks_are_never_interleaved() {
        let scope = Scope::new().await;
        let key = scope.key("c");
        let pool = Arc::new(Pool::new(&scope.addr(), 4));
        let now = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (pool, key, now, peak) = (pool.clone(), key.clone(), now.clone(), peak.clone());
            tasks.push(tokio::spawn(async move {
                let mut conn = pool.get().await.expect("get");
                let inside = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(inside, Ordering::SeqCst);
                conn.call(&["MULTI"]).await.expect("MULTI");
                conn.call(&["INCR", key.as_str()]).await.expect("INCR");
                sleep(Duration::from_millis(20)).await;
                conn.call(&["INCR", key.as_str()]).await.expect("INCR");
                let done = conn.call(&["EXEC"]).await.expect("EXEC");
                now.fetch_sub(1, Ordering::SeqCst);
                done
            }));
        }
        let mut firsts = Vec::new();
        for t in tasks {
            match timeout(Duration::from_secs(10), t).await.expect("завис").expect("задача") {
                Value::Array(items) => match items.as_slice() {
                    [Value::Int(a), Value::Int(b)] => {
                        assert_eq!(*b, a + 1, "две команды одного блока должны идти подряд, а между ними вклинилась чужая: {a} и {b}");
                        firsts.push(*a);
                    }
                    other => panic!("EXEC вернул {other:?}"),
                },
                other => panic!("EXEC вернул {other:?}"),
            }
        }
        firsts.sort_unstable();
        assert_eq!(firsts, (0..20).map(|i| 1 + 2 * i).collect::<Vec<i64>>(), "двадцать блоков по два INCR");
        let peak = peak.load(Ordering::SeqCst);
        assert!(peak >= 2, "пул должен давать работать параллельно, а одновременно держали не больше {peak}");
        assert!(peak <= 4, "одновременно держали {peak} соединений при лимите 4");
    }
}
