// Загрузка аватара уходит PUT-ом, но у пользователя просрочен токен, и обработчик сразу отвечает 401, тело даже не
// трогая. Ответ клиенту ушёл, а несколько килобайт загрузки так и остались в соединении. Соединение keep-alive, и
// сервер честно берётся разбирать следующий запрос... с середины чужого тела. В лучшем случае 400. В худшем в теле
// загрузки лежит строка "GET /admin HTTP/1.1" и сервер её выполнит.
// TODO: Починь serve_connection: после отказа, не читавшего тело, соединение остаётся годным, только если тело
// удалось вычитать и выбросить. Тело больше DRAIN_CAP не глотай: ответь с Connection: close и закрой. Рабочие пути
// (Store, отказ 413) уже правильные.

#![allow(dead_code)]
use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const BODY_LIMIT: usize = 1 << 20;
const DRAIN_CAP: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    None,
    Length(u64),
    Chunked,
}

enum State {
    Length(u64),
    ChunkSize,
    ChunkData(u64),
    ChunkEnd,
    Trailers,
    Done,
}

struct BodyReader<R> {
    reader: R,
    buf: Vec<u8>,
    state: State,
    limit: usize,
    seen: usize,
}

fn invalid<T>(why: &str) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, why.to_string()))
}

impl<R: AsyncRead + Unpin> BodyReader<R> {
    fn new(reader: R, buf: Vec<u8>, framing: Framing, limit: usize) -> io::Result<BodyReader<R>> {
        let state = match framing {
            Framing::None | Framing::Length(0) => State::Done,
            Framing::Length(n) if n > limit as u64 => return invalid("тело больше лимита"),
            Framing::Length(n) => State::Length(n),
            Framing::Chunked => State::ChunkSize,
        };
        Ok(BodyReader { reader, buf, state, limit, seen: 0 })
    }

    fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn into_parts(self) -> (R, Vec<u8>) {
        (self.reader, self.buf)
    }

    async fn more(&mut self) -> io::Result<()> {
        let mut chunk = [0u8; 4096];
        let n = self.reader.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    async fn line(&mut self) -> io::Result<String> {
        loop {
            if let Some(pos) = self.buf.windows(2).position(|w| w == b"\r\n") {
                let line: Vec<u8> = self.buf.drain(..pos + 2).take(pos).collect();
                return String::from_utf8(line).or_else(|_| invalid("строка не в utf-8"));
            }
            if self.buf.len() > 4096 {
                return invalid("слишком длинная строка");
            }
            self.more().await?;
        }
    }

    async fn piece(&mut self, left: u64) -> io::Result<Vec<u8>> {
        if self.buf.is_empty() {
            self.more().await?;
        }
        let n = self.buf.len().min(left as usize).min(4096);
        Ok(self.buf.drain(..n).collect())
    }

    async fn next_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            match self.state {
                State::Done => return Ok(None),
                State::Length(left) => {
                    let piece = self.piece(left).await?;
                    let left = left - piece.len() as u64;
                    self.state = if left == 0 { State::Done } else { State::Length(left) };
                    return Ok(Some(piece));
                }
                State::ChunkSize => {
                    let line = self.line().await?;
                    let size_text = line.split(';').next().unwrap_or("").trim();
                    if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return invalid("плохой размер куска");
                    }
                    let size = u64::from_str_radix(size_text, 16).or_else(|_| invalid("размер куска слишком велик"))?;
                    if size == 0 {
                        self.state = State::Trailers;
                    } else if size > (self.limit - self.seen) as u64 {
                        return invalid("тело больше лимита");
                    } else {
                        self.state = State::ChunkData(size);
                    }
                }
                State::ChunkData(left) => {
                    let piece = self.piece(left).await?;
                    self.seen += piece.len();
                    let left = left - piece.len() as u64;
                    self.state = if left == 0 { State::ChunkEnd } else { State::ChunkData(left) };
                    return Ok(Some(piece));
                }
                State::ChunkEnd => {
                    if !self.line().await?.is_empty() {
                        return invalid("после куска нет CRLF");
                    }
                    self.state = State::ChunkSize;
                }
                State::Trailers => {
                    if self.line().await?.is_empty() {
                        self.state = State::Done;
                    }
                }
            }
        }
    }

    async fn drain(&mut self, cap: usize) -> io::Result<bool> {
        let mut total = 0;
        while let Some(piece) = self.next_chunk().await? {
            total += piece.len();
            if total > cap {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[derive(Debug)]
struct Head {
    method: String,
    target: String,
    minor: u8,
    headers: Vec<(String, String)>,
    len: usize,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn keep_alive(&self) -> bool {
        let close = self.header("connection").is_some_and(|v| v.eq_ignore_ascii_case("close"));
        !close && self.minor == 1
    }
}

fn parse_head(buf: &[u8]) -> Option<Head> {
    let end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let text = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, version) = (first.next()?, first.next()?, first.next()?);
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(n, v)| (n.trim().to_string(), v.trim().to_string())).collect();
    Some(Head { method: method.to_string(), target: target.to_string(), minor: if version == "HTTP/1.0" { 0 } else { 1 }, headers, len: end + 4 })
}

fn body_framing(head: &Head) -> Option<Framing> {
    if head.header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        return Some(Framing::Chunked);
    }
    match head.header("content-length") {
        None => Some(Framing::None),
        Some(v) => v.parse().ok().map(Framing::Length),
    }
}

#[derive(Debug, Clone, Copy)]
enum Action {
    Respond(u16, &'static str),
    Reject(u16),
    Store,
}

async fn respond<S: AsyncWrite + Unpin>(stream: &mut S, status: u16, text: &str, keep_alive: bool) -> io::Result<()> {
    let connection = if keep_alive { "keep-alive" } else { "close" };
    let wire = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n{text}", text.len());
    stream.write_all(wire.as_bytes()).await
}

async fn serve_connection<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, route: fn(&Head) -> Action) -> io::Result<Vec<String>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut served = Vec::new();
    loop {
        let head = loop {
            if let Some(head) = parse_head(&buf) {
                break head;
            }
            let mut chunk = [0u8; 4096];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok(served);
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        buf.drain(..head.len);
        served.push(format!("{} {}", head.method, head.target));
        let mut keep_alive = head.keep_alive();
        let mut consumed = false;
        let (status, text) = match route(&head) {
            Action::Respond(status, text) => (status, text),
            Action::Reject(status) => (status, "rejected"),
            Action::Store => {
                consumed = true;
                let Some(framing) = body_framing(&head) else {
                    respond(&mut stream, 400, "bad framing", false).await?;
                    return Ok(served);
                };
                let taken = std::mem::take(&mut buf);
                let Ok(mut reader) = BodyReader::new(&mut stream, taken, framing, BODY_LIMIT) else {
                    respond(&mut stream, 413, "too large", false).await?;
                    return Ok(served);
                };
                let mut size = 0;
                loop {
                    match reader.next_chunk().await {
                        Ok(Some(piece)) => size += piece.len(),
                        Ok(None) => break,
                        Err(_) => {
                            drop(reader);
                            respond(&mut stream, 413, "too large", false).await?;
                            return Ok(served);
                        }
                    }
                }
                buf = reader.into_parts().1;
                served.push(format!("stored {size}"));
                (201, "stored")
            }
        };
        let _ = consumed;
        respond(&mut stream, status, text, keep_alive).await?;
        if !keep_alive {
            stream.shutdown().await?;
            return Ok(served);
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{DuplexStream, duplex};
    use tokio::time::timeout;

    fn route(head: &Head) -> Action {
        match (head.method.as_str(), head.target.as_str()) {
            ("GET", "/hello") => Action::Respond(200, "hello"),
            ("GET", "/admin") => Action::Respond(200, "secret"),
            ("PUT", "/locked") => Action::Reject(401),
            ("PUT", "/upload") => Action::Store,
            _ => Action::Respond(404, "nope"),
        }
    }

    struct Session {
        client: DuplexStream,
        server: tokio::task::JoinHandle<io::Result<Vec<String>>>,
    }

    fn start() -> Session {
        let (client, server) = duplex(1 << 20);
        Session { client, server: tokio::spawn(serve_connection(server, route)) }
    }

    fn statuses(wire: &str) -> Vec<u16> {
        wire.split("HTTP/1.1 ").skip(1).map(|r| r[..3].parse().unwrap()).collect()
    }

    async fn finish(mut s: Session, wire: &[u8]) -> (String, Vec<String>) {
        s.client.write_all(wire).await.unwrap();
        s.client.shutdown().await.unwrap();
        let mut out = Vec::new();
        timeout(Duration::from_secs(3), s.client.read_to_end(&mut out)).await.expect("сервер не закрыл соединение").unwrap();
        let log = timeout(Duration::from_secs(3), s.server).await.expect("соединение не завершилось").unwrap().unwrap();
        (String::from_utf8_lossy(&out).into_owned(), log)
    }

    #[tokio::test]
    async fn normal_traffic_works() {
        let (out, log) = finish(start(), b"GET /hello HTTP/1.1\r\n\r\nPUT /upload HTTP/1.1\r\nContent-Length: 5\r\n\r\nhelloGET /hello HTTP/1.1\r\n\r\n").await;
        assert_eq!(statuses(&out), [200, 201, 200], "{out}");
        assert_eq!(log, ["GET /hello", "PUT /upload", "stored 5", "GET /hello"]);
    }

    #[tokio::test]
    async fn rejected_upload_must_not_poison_the_next_request() {
        let wire = b"PUT /locked HTTP/1.1\r\nContent-Length: 11\r\n\r\nhello worldGET /hello HTTP/1.1\r\n\r\n";
        let (out, log) = finish(start(), wire).await;
        assert_eq!(statuses(&out), [401, 200], "после отказа соединение должно продолжать работу: {out}");
        assert_eq!(log, ["PUT /locked", "GET /hello"], "тело отказанной загрузки разобрано как запрос");
    }

    #[tokio::test]
    async fn a_request_hidden_inside_the_body_is_never_executed() {
        let hidden = "GET /admin HTTP/1.1\r\n\r\n";
        let wire = format!("PUT /locked HTTP/1.1\r\nContent-Length: {}\r\n\r\n{hidden}GET /hello HTTP/1.1\r\n\r\n", hidden.len());
        let (out, log) = finish(start(), wire.as_bytes()).await;
        assert!(!log.iter().any(|l| l == "GET /admin"), "запрос из тела загрузки выполнился (request smuggling): {log:?}");
        assert!(!out.contains("secret"), "{out}");
        assert_eq!(statuses(&out), [401, 200]);
    }

    #[tokio::test]
    async fn bodies_of_ordinary_answers_are_skipped_too() {
        let wire = b"GET /hello HTTP/1.1\r\nContent-Length: 3\r\n\r\nabcGET /hello HTTP/1.1\r\n\r\n";
        let (out, log) = finish(start(), wire).await;
        assert_eq!(statuses(&out), [200, 200], "{out}");
        assert_eq!(log.len(), 2, "{log:?}");
    }

    #[tokio::test]
    async fn chunked_unread_body_is_skipped() {
        let wire = b"PUT /locked HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n3\r\nGET\r\n0\r\n\r\nGET /hello HTTP/1.1\r\n\r\n";
        let (out, log) = finish(start(), wire).await;
        assert_eq!(statuses(&out), [401, 200], "{out}");
        assert_eq!(log, ["PUT /locked", "GET /hello"]);
    }

    #[tokio::test]
    async fn huge_unread_body_closes_the_connection_instead_of_swallowing_it() {
        let mut s = start();
        s.client.write_all(b"PUT /locked HTTP/1.1\r\nContent-Length: 10000000\r\n\r\n").await.unwrap();
        s.client.write_all(&[b'x'; 200]).await.unwrap();
        let mut out = Vec::new();
        timeout(Duration::from_secs(3), s.client.read_to_end(&mut out))
            .await
            .expect("сервер ждёт 10 МБ ради того, чтобы продолжить: он должен ответить и закрыть соединение")
            .unwrap();
        let out = String::from_utf8_lossy(&out).into_owned();
        assert_eq!(statuses(&out), [401], "{out}");
        assert!(out.to_ascii_lowercase().contains("connection: close"), "клиенту нужно сказать, что соединение закрывается: {out}");
    }

    #[tokio::test]
    async fn too_large_upload_is_refused_and_closed() {
        let mut s = start();
        s.client.write_all(b"PUT /upload HTTP/1.1\r\nContent-Length: 2000000\r\n\r\nabc").await.unwrap();
        let mut out = Vec::new();
        timeout(Duration::from_secs(3), s.client.read_to_end(&mut out)).await.expect("слишком большая загрузка должна закрывать соединение").unwrap();
        assert_eq!(statuses(&String::from_utf8_lossy(&out)), [413]);
    }
}
