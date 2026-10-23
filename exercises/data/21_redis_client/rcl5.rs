// Приложение получает адрес Redis одной строкой из конфига: `redis://:пароль@хост:порт/номер_базы`.
// Клиент должен сам разобрать её, при подключении войти по паролю (AUTH) и выбрать базу (SELECT).
// Кое-что в адресе необязательно, а пароль может содержать символы, которые в адресе имеют смысл.
// TODO: Напиши `parse_url` (в ошибке причина текстом) и `connect`, которая подключается и выполняет
// AUTH и SELECT только там, где они нужны. Отказ сервера на AUTH это ошибка подключения.

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

#[derive(Debug, PartialEq)]
struct ConnectionInfo {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    db: u32,
}

fn parse_url(url: &str) -> Result<ConnectionInfo, String> {
    todo!()
}

async fn connect(url: &str) -> io::Result<Conn> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::{self, FakeServer, Step};
    use std::time::Duration;
    use tokio::time::timeout;

    fn reply(text: &str) -> Vec<Step> {
        vec![Step::Write(text.as_bytes().to_vec())]
    }

    fn info(host: &str, port: u16, username: Option<&str>, password: Option<&str>, db: u32) -> ConnectionInfo {
        ConnectionInfo { host: host.into(), port, username: username.map(String::from), password: password.map(String::from), db }
    }

    fn sent(list: &[&[&str]]) -> Vec<Vec<String>> {
        list.iter().map(|c| c.iter().map(|s| s.to_string()).collect()).collect()
    }

    #[test]
    fn full_url() {
        assert_eq!(parse_url("redis://bob:secret@cache.local:6380/3"), Ok(info("cache.local", 6380, Some("bob"), Some("secret"), 3)), "все части адреса");
    }

    #[test]
    fn defaults() {
        assert_eq!(parse_url("redis://localhost"), Ok(info("localhost", 6379, None, None, 0)), "порт 6379, база 0, без пароля");
        assert_eq!(parse_url("redis://localhost:6379/"), Ok(info("localhost", 6379, None, None, 0)), "косая черта без номера это база 0");
    }

    #[test]
    fn password_without_user() {
        assert_eq!(parse_url("redis://:s3cret@10.0.0.5/2"), Ok(info("10.0.0.5", 6379, None, Some("s3cret"), 2)), "пустое имя значит «без имени»");
    }

    #[test]
    fn special_characters_in_password() {
        assert_eq!(parse_url("redis://:p%40ss%3Aw%2Ford@host:1"), Ok(info("host", 1, None, Some("p@ss:w/ord"), 0)), "спецсимволы в пароле приходят закодированными (%40 это @)");
    }

    #[test]
    fn bad_urls_are_refused() {
        for bad in ["http://host", "redis://", "redis://host:abc", "redis://host:70000", "redis://host/xyz", "redis://:pw@:6379", "redis://nocolon@host", "localhost:6379"] {
            assert!(parse_url(bad).is_err(), "адрес {bad:?} неправильный, а разобран без ошибки");
        }
    }

    #[tokio::test]
    async fn auth_and_select_in_order() {
        let server = FakeServer::start(|_, _| reply("+OK\r\n")).await;
        let url = format!("redis://:s3cret@{}/2", server.addr());
        timeout(Duration::from_secs(3), connect(&url)).await.expect("завис").expect("подключение");
        assert_eq!(server.commands(), sent(&[&["AUTH", "s3cret"], &["SELECT", "2"]]), "сначала AUTH (без него SELECT откажет), потом SELECT");
    }

    #[tokio::test]
    async fn auth_with_user_name() {
        let server = FakeServer::start(|_, _| reply("+OK\r\n")).await;
        let url = format!("redis://bob:p%40ss@{}", server.addr());
        timeout(Duration::from_secs(3), connect(&url)).await.expect("завис").expect("подключение");
        assert_eq!(server.commands(), sent(&[&["AUTH", "bob", "p@ss"]]), "AUTH с именем и раскодированным паролем, а SELECT не нужен: база 0");
    }

    #[tokio::test]
    async fn nothing_extra_by_default() {
        let server = FakeServer::start(|_, _| reply("+OK\r\n")).await;
        timeout(Duration::from_secs(3), connect(&format!("redis://{}", server.addr()))).await.expect("завис").expect("подключение");
        assert!(server.commands().is_empty(), "без пароля и базы клиент ничего не отправляет при подключении, а отправил {:?}", server.commands());
    }

    #[tokio::test]
    async fn refused_password_fails_the_connection() {
        let server = FakeServer::start(|_, cmd| if cmd[0] == "AUTH" { reply("-WRONGPASS invalid username-password pair\r\n") } else { reply("+OK\r\n") }).await;
        let url = format!("redis://:bad@{}/4", server.addr());
        let err = timeout(Duration::from_secs(3), connect(&url)).await.expect("завис").err().expect("отказ на AUTH должен сорвать подключение");
        assert!(err.to_string().contains("WRONGPASS"), "в ошибке должен быть ответ сервера: {err}");
        assert_eq!(server.commands().len(), 1, "после отказа SELECT отправлять нельзя");
    }

    #[tokio::test]
    async fn real_redis_selects_the_database() {
        let url = format!("{}/9", redis::url().trim_end_matches('/'));
        let mut conn = timeout(Duration::from_secs(3), connect(&url)).await.expect("завис").expect("подключение к базе 9");
        match conn.call(&["CLIENT", "INFO"]).await.expect("CLIENT INFO") {
            Value::Bulk(info) => assert!(String::from_utf8_lossy(&info).contains(" db=9 "), "соединение должно работать в базе 9: {}", String::from_utf8_lossy(&info)),
            other => panic!("CLIENT INFO вернул {other:?}"),
        }
    }
}
