// Про pipelining все говорят «быстрее в десятки раз», и на слово верить не надо. Напиши замер: один и тот же
// объём работы (`n` записей SET) делается двумя способами, по одной команде с ожиданием ответа и пачками
// по `batch` штук. Замеряй только саму работу, без подключения. После замера обе серии должны оставить в
// Redis все ключи, иначе быстрым «окажется» то, что ничего не сделало.
// TODO: Напиши `measure` и `ops_per_sec`. Ключи: `<prefix>:seq:<i>` и `<prefix>:pipe:<i>`, значение равно `i`.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::time::{Duration, Instant};

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
        let mut wire = Vec::new();
        for cmd in cmds {
            wire.extend(encode_command(cmd.as_slice()));
        }
        self.stream.write_all(&wire).await?;
        let mut replies = Vec::with_capacity(cmds.len());
        while replies.len() < cmds.len() {
            match decode(&mut self.buf) {
                Ok(Some(value)) => replies.push(value),
                Ok(None) => {
                    if self.stream.read_buf(&mut self.buf).await? == 0 {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "сервер закрыл соединение"));
                    }
                }
                Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "ответ не похож на RESP")),
            }
        }
        Ok(replies)
    }
}

struct Measure {
    sequential: Duration,
    pipelined: Duration,
}

async fn measure(conn: &mut Conn, prefix: &str, n: usize, batch: usize) -> io::Result<Measure> {
    todo!()
}

fn ops_per_sec(n: usize, took: Duration) -> f64 {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;
    use tokio::time::timeout;

    #[tokio::test]
    async fn pipeline_beats_one_by_one() {
        let scope = Scope::new().await;
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        let m = timeout(Duration::from_secs(9), measure(&mut conn, scope.prefix(), 10_000, 1000)).await.expect("замер не уложился в 9 секунд").expect("замер");
        println!(
            "по одной: {:?} ({:.0} оп/с), пачками по 1000: {:?} ({:.0} оп/с)",
            m.sequential,
            ops_per_sec(10_000, m.sequential),
            m.pipelined,
            ops_per_sec(10_000, m.pipelined)
        );
        assert!(m.pipelined * 3 < m.sequential, "пачки должны быть минимум втрое быстрее: по одной {:?}, пачками {:?}", m.sequential, m.pipelined);
    }

    #[tokio::test]
    async fn both_series_really_wrote_everything() {
        let scope = Scope::new().await;
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        measure(&mut conn, scope.prefix(), 2500, 1000).await.expect("замер");
        for kind in ["seq", "pipe"] {
            for i in [0usize, 999, 1000, 2499] {
                let key = format!("{}:{kind}:{i}", scope.prefix());
                let got = conn.call(&["GET", key.as_str()]).await.expect("GET");
                assert_eq!(got, Value::Bulk(i.to_string().into_bytes()), "ключ {key} должен хранить {i}: серия {kind} записала не всё (в том числе хвост неполной пачки)");
            }
        }
    }

    #[test]
    fn rate_is_operations_over_seconds() {
        assert!((ops_per_sec(1000, Duration::from_millis(500)) - 2000.0).abs() < 1e-6, "тысяча операций за полсекунды это 2000 в секунду");
        assert!((ops_per_sec(5, Duration::from_secs(10)) - 0.5).abs() < 1e-9, "пять за десять секунд это 0,5 в секунду");
    }
}
