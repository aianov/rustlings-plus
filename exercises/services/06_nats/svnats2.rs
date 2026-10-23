// Пора поговорить с настоящим брокером без async-nats, на голом TcpStream. Разбор операций
// (parse_op) уже готов из прошлой задачи. Нужен клиент: подключиться, подписаться, опубликовать
// и принять сообщение. Брокер сам присылает PING, и клиент обязан ответить, иначе его отключат.
// Цель: connect возвращается, когда сервер принял CONNECT; subscribe возвращается, когда сервер
// уже знает про подписку (публикация другого клиента сразу после этого не пропадёт); сообщения,
// пришедшие по дороге, не теряются; next_message отдаёт их по порядку.


use std::collections::VecDeque;
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};

fn main() {}

#[derive(Debug, PartialEq)]
pub enum Op {
    Info(String),
    Ping,
    Pong,
    Ok,
    Err(String),
    Msg { subject: String, sid: u64, reply: Option<String>, payload: Vec<u8> },
    HMsg { subject: String, sid: u64, reply: Option<String>, headers: Vec<u8>, payload: Vec<u8> },
}

#[derive(Debug, PartialEq)]
pub struct ProtoError(pub String);

pub const MAX_LINE: usize = 4096;
pub const MAX_BODY: usize = 8 * 1024 * 1024;

pub fn parse_op(buf: &[u8]) -> Result<Option<(Op, usize)>, ProtoError> {
    let Some(end) = buf.windows(2).position(|w| w == b"\r\n") else {
        if buf.len() > MAX_LINE {
            return Err(ProtoError("строка управления длиннее лимита".to_string()));
        }
        return Ok(None);
    };
    if end > MAX_LINE {
        return Err(ProtoError("строка управления длиннее лимита".to_string()));
    }
    let line = std::str::from_utf8(&buf[..end]).map_err(|_| ProtoError("строка управления не UTF-8".to_string()))?;
    let head = end + 2;
    let mut words = line.split_whitespace();
    let verb = words.next().unwrap_or("").to_ascii_uppercase();
    let args: Vec<&str> = words.collect();
    let rest = line.get(verb.len()..).unwrap_or("").trim();
    match verb.as_str() {
        "PING" => Ok(Some((Op::Ping, head))),
        "PONG" => Ok(Some((Op::Pong, head))),
        "+OK" => Ok(Some((Op::Ok, head))),
        "-ERR" => Ok(Some((Op::Err(rest.trim_matches('\'').to_string()), head))),
        "INFO" => Ok(Some((Op::Info(rest.to_string()), head))),
        "MSG" => {
            let (subject, sid, reply, len) = match args.as_slice() {
                [subject, sid, len] => (subject, sid, None, len),
                [subject, sid, reply, len] => (subject, sid, Some(reply.to_string()), len),
                _ => return Err(ProtoError(format!("MSG: неверное число полей в {line:?}"))),
            };
            let (len, sid) = (number(len)?, number(sid)? as u64);
            let Some(body) = body_at(buf, head, len)? else { return Ok(None) };
            let op = Op::Msg { subject: subject.to_string(), sid, reply, payload: body.to_vec() };
            Ok(Some((op, head + len + 2)))
        }
        "HMSG" => {
            let (subject, sid, reply, hlen, tlen) = match args.as_slice() {
                [subject, sid, hlen, tlen] => (subject, sid, None, hlen, tlen),
                [subject, sid, reply, hlen, tlen] => (subject, sid, Some(reply.to_string()), hlen, tlen),
                _ => return Err(ProtoError(format!("HMSG: неверное число полей в {line:?}"))),
            };
            let (hlen, tlen, sid) = (number(hlen)?, number(tlen)?, number(sid)? as u64);
            if hlen > tlen {
                return Err(ProtoError("HMSG: заголовки длиннее всего сообщения".to_string()));
            }
            let Some(body) = body_at(buf, head, tlen)? else { return Ok(None) };
            let op = Op::HMsg {
                subject: subject.to_string(),
                sid,
                reply,
                headers: body[..hlen].to_vec(),
                payload: body[hlen..].to_vec(),
            };
            Ok(Some((op, head + tlen + 2)))
        }
        other => Err(ProtoError(format!("неизвестная операция {other:?}"))),
    }
}

fn number(word: &str) -> Result<usize, ProtoError> {
    word.parse::<usize>().map_err(|_| ProtoError(format!("ожидали число, пришло {word:?}")))
}

fn body_at(buf: &[u8], head: usize, len: usize) -> Result<Option<&[u8]>, ProtoError> {
    if len > MAX_BODY {
        return Err(ProtoError(format!("сообщение в {len} байт больше лимита {MAX_BODY}")));
    }
    let end = head + len;
    if buf.len() < end + 2 {
        return Ok(None);
    }
    if &buf[end..end + 2] != b"\r\n" {
        return Err(ProtoError("после тела нет \\r\\n: длина в строке управления не сходится с данными".to_string()));
    }
    Ok(Some(&buf[head..end]))
}

#[derive(Debug, PartialEq)]
pub struct Incoming {
    pub subject: String,
    pub sid: u64,
    pub reply: Option<String>,
    pub payload: Vec<u8>,
}

pub struct RawClient {
    stream: TcpStream,
    buf: Vec<u8>,
    pending: VecDeque<Incoming>,
}

fn broken(text: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text.into())
}

impl RawClient {
    pub async fn connect(addr: impl ToSocketAddrs) -> io::Result<RawClient> {
        // TODO: прочитай INFO, отправь CONNECT и убедись, что сервер принял его
        todo!()
    }

    pub async fn subscribe(&mut self, subject: &str, sid: u64) -> io::Result<()> {
        todo!()
    }

    pub async fn publish(&mut self, subject: &str, payload: &[u8]) -> io::Result<()> {
        todo!()
    }

    pub async fn next_message(&mut self) -> io::Result<Incoming> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    async fn fake_server<F, Fut>(script: F) -> String
    where
        F: FnOnce(TcpStream) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
        let addr = listener.local_addr().expect("адрес").to_string();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("клиент");
            script(stream).await;
        });
        addr
    }

    async fn read_until(stream: &mut TcpStream, seen: &mut Vec<u8>, needle: &[u8]) {
        let found = |seen: &Vec<u8>| seen.windows(needle.len()).any(|w| w == needle);
        while !found(seen) {
            let mut chunk = [0u8; 1024];
            let n = timeout(Duration::from_secs(3), stream.read(&mut chunk)).await.expect("клиент молчит").expect("чтение");
            assert!(n > 0, "клиент закрыл соединение раньше времени, получено {:?}", String::from_utf8_lossy(seen));
            seen.extend_from_slice(&chunk[..n]);
        }
    }

    async fn greet(stream: &mut TcpStream) -> Vec<u8> {
        stream.write_all(b"INFO {\"server_id\":\"fake\",\"max_payload\":1048576}\r\n").await.expect("INFO");
        let mut seen = Vec::new();
        read_until(stream, &mut seen, b"PING\r\n").await;
        stream.write_all(b"PONG\r\n").await.expect("PONG");
        seen
    }

    #[tokio::test]
    async fn connect_sends_connect_then_ping_and_waits_for_pong() {
        let answered = Arc::new(AtomicBool::new(false));
        let flag = answered.clone();
        let addr = fake_server(move |mut stream| async move {
            stream.write_all(b"INFO {\"server_id\":\"fake\"}\r\n").await.unwrap();
            let mut seen = Vec::new();
            read_until(&mut stream, &mut seen, b"PING\r\n").await;
            assert!(seen.starts_with(b"CONNECT {"), "первой строкой клиент должен прислать CONNECT, пришло {:?}", String::from_utf8_lossy(&seen));
            tokio::time::sleep(Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
            stream.write_all(b"PONG\r\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
        })
        .await;
        RawClient::connect(addr).await.expect("подключение");
        assert!(answered.load(Ordering::SeqCst), "connect вернулся раньше, чем сервер ответил PONG: сервер мог ещё отвергнуть CONNECT");
    }

    #[tokio::test]
    async fn server_error_on_connect_is_reported() {
        let addr = fake_server(|mut stream| async move {
            stream.write_all(b"INFO {}\r\n").await.unwrap();
            let mut seen = Vec::new();
            read_until(&mut stream, &mut seen, b"PING\r\n").await;
            stream.write_all(b"-ERR 'Authorization Violation'\r\n").await.unwrap();
        })
        .await;
        let error = RawClient::connect(addr).await.err().expect("-ERR от сервера должен стать ошибкой connect");
        assert!(error.to_string().contains("Authorization Violation"), "в ошибке должен быть текст сервера: {error}");
    }

    #[tokio::test]
    async fn subscribe_does_not_return_before_the_server_answers() {
        let addr = fake_server(|mut stream| async move {
            greet(&mut stream).await;
            let mut seen = Vec::new();
            read_until(&mut stream, &mut seen, b"SUB a.> 7\r\n").await;
            tokio::time::sleep(Duration::from_millis(400)).await;
        })
        .await;
        let mut client = RawClient::connect(addr).await.expect("подключение");
        let early = timeout(Duration::from_millis(150), client.subscribe("a.>", 7)).await;
        assert!(early.is_err(), "subscribe вернулся, пока сервер молчит: после SUB нужен PING и ожидание PONG, иначе подписка может оказаться не зарегистрированной");
    }

    #[tokio::test]
    async fn subscribe_waits_and_publish_frames_the_payload_by_length() {
        let (tx, rx) = oneshot::channel();
        let addr = fake_server(|mut stream| async move {
            greet(&mut stream).await;
            let mut seen = Vec::new();
            read_until(&mut stream, &mut seen, b"PING\r\n").await;
            stream.write_all(b"PONG\r\n").await.unwrap();
            read_until(&mut stream, &mut seen, b"PUB a.b 4\r\na\r\nb\r\n").await;
            let _ = tx.send(seen);
        })
        .await;
        let mut client = RawClient::connect(addr).await.expect("подключение");
        client.subscribe("a.>", 7).await.expect("подписка");
        client.publish("a.b", b"a\r\nb").await.expect("публикация");
        let wire = String::from_utf8(timeout(Duration::from_secs(3), rx).await.expect("сервер ждал").expect("канал")).expect("UTF-8");
        assert!(wire.contains("SUB a.> 7\r\n"), "строка подписки: SUB <тема> <sid>, получено {wire:?}");
        assert!(wire.contains("PUB a.b 4\r\na\r\nb\r\n"), "публикация: PUB <тема> <длина>, затем тело и \\r\\n, получено {wire:?}");
    }

    #[tokio::test]
    async fn server_ping_gets_a_pong_and_does_not_hide_messages() {
        let (tx, rx) = oneshot::channel();
        let addr = fake_server(|mut stream| async move {
            let mut seen = greet(&mut stream).await;
            seen.clear();
            stream.write_all(b"PING\r\nMSG x.y 1 2\r\nhi\r\nPING\r\nMSG x.y 1 3\r\nyou\r\n").await.unwrap();
            read_until(&mut stream, &mut seen, b"PONG\r\nPONG\r\n").await;
            let _ = tx.send(());
        })
        .await;
        let mut client = RawClient::connect(addr).await.expect("подключение");
        let first = client.next_message().await.expect("первое сообщение");
        assert_eq!((first.subject.as_str(), first.payload.as_slice()), ("x.y", &b"hi"[..]), "первое сообщение");
        let second = client.next_message().await.expect("второе сообщение");
        assert_eq!(second.payload, b"you", "второе сообщение идёт после второго PING");
        timeout(Duration::from_secs(3), rx).await.expect("сервер не получил PONG").expect("канал");
    }

    #[tokio::test]
    async fn message_that_arrives_before_the_pong_is_kept() {
        let addr = fake_server(|mut stream| async move {
            stream.write_all(b"INFO {}\r\n").await.unwrap();
            let mut seen = Vec::new();
            read_until(&mut stream, &mut seen, b"PING\r\n").await;
            stream.write_all(b"PONG\r\n").await.unwrap();
            seen.clear();
            read_until(&mut stream, &mut seen, b"SUB x.y 1\r\nPING\r\n").await;
            stream.write_all(b"MSG x.y 1 5\r\nearly\r\nPONG\r\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
        })
        .await;
        let mut client = RawClient::connect(addr).await.expect("подключение");
        client.subscribe("x.y", 1).await.expect("подписка");
        let message = timeout(Duration::from_secs(2), client.next_message()).await.expect("ждём сообщение").expect("сообщение");
        assert_eq!(message.payload, b"early", "сообщение пришло между SUB и PONG, оно не должно потеряться");
    }

    fn nats_addr() -> String {
        let url = course_support::nats::url();
        url.strip_prefix("nats://").unwrap_or(&url).to_string()
    }

    #[tokio::test]
    async fn two_clients_talk_through_the_real_broker() {
        let scope = course_support::nats::Scope::new();
        let subject = scope.subject("greet");
        let mut listener = RawClient::connect(nats_addr()).await.expect("подключение слушателя");
        listener.subscribe(&scope.wildcard(), 11).await.expect("подписка");
        let mut talker = RawClient::connect(nats_addr()).await.expect("подключение отправителя");
        talker.publish(&subject, b"hello\r\nworld").await.expect("публикация");
        let got = timeout(Duration::from_secs(3), listener.next_message()).await.expect("сообщение не пришло").expect("чтение");
        assert_eq!(got.subject, subject, "тема сообщения");
        assert_eq!(got.sid, 11, "sid нашей подписки");
        assert_eq!(got.payload, b"hello\r\nworld", "тело дошло байт в байт");
    }

    #[tokio::test]
    async fn many_messages_arrive_in_order() {
        let scope = course_support::nats::Scope::new();
        let subject = scope.subject("count");
        let mut listener = RawClient::connect(nats_addr()).await.expect("подключение слушателя");
        listener.subscribe(&subject, 1).await.expect("подписка");
        let mut talker = RawClient::connect(nats_addr()).await.expect("подключение отправителя");
        for i in 0..200 {
            talker.publish(&subject, format!("m{i}").as_bytes()).await.expect("публикация");
        }
        for i in 0..200 {
            let got = timeout(Duration::from_secs(3), listener.next_message()).await.expect("сообщение не пришло").expect("чтение");
            assert_eq!(got.payload, format!("m{i}").as_bytes(), "сообщения от одного отправителя приходят в порядке отправки");
        }
    }
}
