use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

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
    reader: OwnedReadHalf,
    writer: BufWriter<OwnedWriteHalf>,
    buf: BytesMut,
}

impl Conn {
    async fn connect(addr: &str) -> io::Result<Conn> {
        let (reader, writer) = TcpStream::connect(addr).await?.into_split();
        Ok(Conn { reader, writer: BufWriter::new(writer), buf: BytesMut::new() })
    }

    async fn read_reply(&mut self) -> io::Result<Value> {
        loop {
            match decode(&mut self.buf) {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => {}
                Err(_) => return Err(io::Error::new(io::ErrorKind::InvalidData, "ответ не похож на RESP")),
            }
            if self.reader.read_buf(&mut self.buf).await? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "сервер закрыл соединение"));
            }
        }
    }

    async fn call<S: AsRef<[u8]>>(&mut self, args: &[S]) -> io::Result<Value> {
        self.writer.write_all(&encode_command(args)).await?;
        self.writer.flush().await?;
        self.read_reply().await
    }

    async fn pipeline(&mut self, cmds: &[Vec<String>]) -> io::Result<Vec<Value>> {
        for cmd in cmds {
            self.writer.write_all(&encode_command(cmd.as_slice())).await?;
        }
        self.writer.flush().await?;
        let mut replies = Vec::with_capacity(cmds.len());
        for _ in cmds {
            replies.push(self.read_reply().await?);
        }
        Ok(replies)
    }

    async fn quit(&mut self) -> io::Result<()> {
        self.writer.write_all(&encode_command(&["QUIT"])).await?;
        self.writer.flush().await?;
        self.read_reply().await?;
        Ok(())
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{self, FakeServer, Scope, Step};
    use std::time::Duration;
    use tokio::time::timeout;

    fn reply(text: &str) -> Vec<Step> {
        vec![Step::Write(text.as_bytes().to_vec())]
    }

    fn cmd(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn single_call_works() {
        let scope = Scope::new().await;
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        let got = timeout(Duration::from_secs(3), conn.call(&["PING"])).await.expect("завис").expect("PING");
        assert_eq!(got, Value::Simple("PONG".into()), "обычный вызов");
    }

    #[tokio::test]
    async fn small_pipeline() {
        let mut conn = Conn::connect(&redis::addr()).await.expect("подключение");
        let got = timeout(Duration::from_secs(2), conn.pipeline(&[cmd(&["PING"]), cmd(&["PING"])]))
            .await
            .expect("клиент завис: пачка осталась в буфере записи, до сервера она не дошла")
            .expect("pipeline");
        assert_eq!(got, vec![Value::Simple("PONG".into()); 2], "два ответа");
    }

    #[tokio::test]
    async fn big_pipeline() {
        let scope = Scope::new().await;
        let mut conn = Conn::connect(&scope.addr()).await.expect("подключение");
        let big = "x".repeat(100_000);
        let cmds: Vec<Vec<String>> = (0..5).map(|i| vec!["SET".to_string(), scope.key(&format!("big{i}")), big.clone()]).collect();
        let got = timeout(Duration::from_secs(3), conn.pipeline(&cmds)).await.expect("клиент завис: хвост пачки остался в буфере записи").expect("pipeline");
        assert_eq!(got.len(), 5, "пять ответов");
    }

    #[tokio::test]
    async fn quit_reaches_the_server() {
        let server = FakeServer::start(|_, _| reply("+OK\r\n")).await;
        let mut conn = Conn::connect(&server.addr()).await.expect("подключение");
        timeout(Duration::from_secs(2), conn.quit()).await.expect("QUIT остался в буфере записи, сервер его не получил").expect("QUIT");
        assert_eq!(server.commands(), vec![vec!["QUIT".to_string()]], "сервер получил QUIT");
    }
}
