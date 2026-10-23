// Все кирпичи готовы (адрес, запрос, голова, тело, ответ), осталось сложить из них то, чем пользуются каждый
// день: get_json и post_json. Главное тут не соединить провода, а разделить ошибки. «Сервер недоступен»,
// «сервер ответил 404» и «сервер ответил мусором» это три разных ситуации, и вызывающий реагирует на них по-разному.
// TODO: Допиши send (соединение на запрос, Connection: close), get_json и post_json. Успех это 2xx и верный JSON.

#![allow(dead_code)]
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;

type Headers = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
struct Url {
    host: String,
    port: u16,
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UrlError {
    Scheme,
    UserInfo,
    Host,
    Port,
    BadChar,
}

impl Url {
    fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == 80 { host } else { format!("{host}:{}", self.port) }
    }
}

fn parse_url(input: &str) -> Result<Url, UrlError> {
    if input.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UrlError::BadChar);
    }
    let (scheme, rest) = input.split_once("://").ok_or(UrlError::Scheme)?;
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(UrlError::Scheme);
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') {
        return Err(UrlError::UserInfo);
    }
    let (host, port) = match authority.strip_prefix('[') {
        Some(inner) => {
            let (host, after) = inner.split_once(']').ok_or(UrlError::Host)?;
            if host.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(UrlError::Host);
            }
            match after {
                "" => (host, None),
                _ => (host, Some(after.strip_prefix(':').ok_or(UrlError::Host)?)),
            }
        }
        None => match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    if host.is_empty() || (!authority.starts_with('[') && host.contains(':')) {
        return Err(UrlError::Host);
    }
    let port = match port {
        None => 80,
        Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => match p.parse::<u16>() {
            Ok(0) | Err(_) => return Err(UrlError::Port),
            Ok(n) => n,
        },
        Some(_) => return Err(UrlError::Port),
    };
    let tail = tail.split('#').next().unwrap_or("");
    let path = match tail {
        "" => "/".to_string(),
        t if t.starts_with('?') => format!("/{t}"),
        t => t.to_string(),
    };
    Ok(Url { host: host.to_ascii_lowercase(), port, path })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum RequestError {
    BadMethod,
    BadHeader,
    BadTarget,
}

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn build_request(method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>) -> Result<Vec<u8>, RequestError> {
    if !is_token(method) {
        return Err(RequestError::BadMethod);
    }
    if !url.path.starts_with('/') || url.path.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(RequestError::BadTarget);
    }
    let mut out = format!("{method} {} HTTP/1.1\r\nHost: {}\r\n", url.path, url.authority());
    for (name, value) in headers {
        if !is_token(name) || value.contains(['\r', '\n', '\0']) {
            return Err(RequestError::BadHeader);
        }
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_str(), "host" | "content-length" | "transfer-encoding") {
            continue;
        }
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    match body {
        Some(b) => out.push_str(&format!("Content-Length: {}\r\n", b.len())),
        None if matches!(method, "POST" | "PUT" | "PATCH") => out.push_str("Content-Length: 0\r\n"),
        None => {}
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    if let Some(b) = body {
        bytes.extend_from_slice(b);
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq)]
struct Head {
    minor: u8,
    status: u16,
    reason: String,
    headers: Headers,
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadError {
    StatusLine,
    Version,
    Status,
    Header,
    TooLarge,
}

const MAX_HEAD: usize = 16 * 1024;

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.all("connection") {
            for token in value.split(',') {
                close |= token.trim().eq_ignore_ascii_case("close");
                keep |= token.trim().eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.minor == 1)
    }
}

fn parse_head(buf: &[u8]) -> Result<Option<Head>, HeadError> {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() > MAX_HEAD { Err(HeadError::TooLarge) } else { Ok(None) };
    };
    let len = end + 4;
    if len > MAX_HEAD {
        return Err(HeadError::TooLarge);
    }
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| HeadError::Header)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("");
    let (version, rest) = first.split_once(' ').ok_or(HeadError::StatusLine)?;
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        v if v.starts_with("HTTP/") => return Err(HeadError::Version),
        _ => return Err(HeadError::StatusLine),
    };
    let (code, reason) = rest.split_once(' ').unwrap_or((rest, ""));
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HeadError::Status);
    }
    let status: u16 = code.parse().map_err(|_| HeadError::Status)?;
    if status < 100 {
        return Err(HeadError::Status);
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.starts_with([' ', '\t']) {
            return Err(HeadError::Header);
        }
        let (name, value) = line.split_once(':').ok_or(HeadError::Header)?;
        if !is_token(name) || value.contains(['\r', '\n']) {
            return Err(HeadError::Header);
        }
        headers.push((name.to_string(), value.trim_matches([' ', '\t']).to_string()));
    }
    Ok(Some(Head { minor, status, reason: reason.to_string(), headers, len }))
}

#[derive(Debug, Clone, PartialEq)]
struct Body {
    bytes: Vec<u8>,
    reusable: bool,
}

async fn fill<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<usize> {
    let mut chunk = [0u8; 4096];
    let n = reader.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    Ok(n)
}

fn invalid<T>(why: &str) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, why.to_string()))
}

async fn take_line<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<String> {
    loop {
        if let Some(pos) = buf.windows(2).position(|w| w == b"\r\n") {
            let line: Vec<u8> = buf.drain(..pos + 2).take(pos).collect();
            return String::from_utf8(line).or_else(|_| invalid("строка не в utf-8"));
        }
        if buf.len() > 4096 {
            return invalid("слишком длинная строка");
        }
        if fill(reader, buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }
}

async fn read_body<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>, head: &Head, max: usize) -> io::Result<Body> {
    let encodings = head.all("transfer-encoding");
    let chunked = encodings.last().is_some_and(|v| v.rsplit(',').next().is_some_and(|t| t.trim().eq_ignore_ascii_case("chunked")));
    let mut lengths = Vec::new();
    for value in head.all("content-length") {
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return invalid("плохой Content-Length");
            }
            lengths.push(part.parse::<usize>().or_else(|_| invalid("Content-Length слишком велик"))?);
        }
    }
    if lengths.windows(2).any(|w| w[0] != w[1]) {
        return invalid("разные Content-Length");
    }
    if chunked {
        let mut bytes = Vec::new();
        loop {
            let line = take_line(reader, buf).await?;
            let size_text = line.split(';').next().unwrap_or("").trim();
            if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                return invalid("плохой размер куска");
            }
            let size = usize::from_str_radix(size_text, 16).or_else(|_| invalid("размер куска слишком велик"))?;
            if size == 0 {
                while !take_line(reader, buf).await?.is_empty() {}
                return Ok(Body { bytes, reusable: lengths.is_empty() });
            }
            if size > max - bytes.len().min(max) {
                return invalid("тело больше лимита");
            }
            while buf.len() < size + 2 {
                if fill(reader, buf).await? == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            bytes.extend_from_slice(&buf[..size]);
            if &buf[size..size + 2] != b"\r\n" {
                return invalid("после куска нет CRLF");
            }
            buf.drain(..size + 2);
        }
    }
    if let Some(&n) = lengths.first().filter(|_| encodings.is_empty()) {
        if n > max {
            return invalid("тело больше лимита");
        }
        while buf.len() < n {
            if fill(reader, buf).await? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }
        return Ok(Body { bytes: buf.drain(..n).collect(), reusable: true });
    }
    loop {
        if buf.len() > max {
            return invalid("тело больше лимита");
        }
        if fill(reader, buf).await? == 0 {
            return Ok(Body { bytes: std::mem::take(buf), reusable: false });
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    headers: Headers,
    body: Vec<u8>,
    reusable: bool,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn head_error(e: HeadError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("плохая голова ответа: {e:?}"))
}

async fn read_response<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>, method: &str, max: usize) -> io::Result<Response> {
    let head = loop {
        if let Some(head) = parse_head(buf).map_err(head_error)? {
            buf.drain(..head.len);
            if (100..200).contains(&head.status) && head.status != 101 {
                continue;
            }
            break head;
        }
        if fill(stream, buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    };
    let bodiless = method == "HEAD" || head.status == 204 || head.status == 304 || head.status == 101;
    let (bytes, reusable) = if bodiless {
        (Vec::new(), true)
    } else {
        let body = read_body(stream, buf, &head, max).await?;
        (body.bytes, body.reusable)
    };
    Ok(Response { status: head.status, reusable: reusable && head.keep_alive() && buf.is_empty(), headers: head.headers, body: bytes })
}

#[derive(Debug)]
enum ClientError {
    Url(UrlError),
    Request(RequestError),
    Transport(io::Error),
    Status { code: u16, body: String },
    Decode(String),
}

struct Client {
    max_body: usize,
}

impl Client {
    fn new() -> Client {
        Client { max_body: 1 << 20 }
    }

    async fn send(&self, method: &str, url: &str, headers: &[(String, String)], body: Option<&[u8]>) -> Result<Response, ClientError> {
        let _ = (method, url, headers, body);
        todo!()
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, ClientError> {
        let _ = url;
        todo!()
    }

    async fn post_json<B: Serialize, T: DeserializeOwned>(&self, url: &str, body: &B) -> Result<T, ClientError> {
        let _ = (url, body);
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new();
    let result: Result<serde_json::Value, ClientError> = client.get_json("http://127.0.0.1:1/").await;
    println!("{result:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::sync::Mutex;
    use std::future::Future;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::net::{TcpListener, TcpStream};

    async fn read_request_head(stream: &mut TcpStream) -> Option<String> {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.ok()? == 0 {
                return None;
            }
            buf.push(byte[0]);
        }
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    async fn read_request_body(stream: &mut TcpStream, head: &str) -> Vec<u8> {
        let len = head
            .lines()
            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
            .unwrap_or(0);
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).await.unwrap();
        body
    }

    fn spawn_server<F, Fut>(script: F) -> (SocketAddr, Arc<AtomicUsize>)
    where
        F: Fn(usize, TcpStream) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = accepted.clone();
        tokio::spawn(async move {
            let listener = TcpListener::from_std(listener).unwrap();
            loop {
                let Ok((stream, _)) = listener.accept().await else { return };
                let n = counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(script(n, stream));
            }
        });
        (addr, accepted)
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct User {
        name: String,
        age: u32,
    }

    type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

    fn server_replying(wire: &'static str) -> (String, Seen) {
        let seen: Seen = Arc::default();
        let log = seen.clone();
        let (addr, _) = spawn_server(move |_, mut stream| {
            let log = log.clone();
            async move {
                let Some(head) = read_request_head(&mut stream).await else { return };
                let body = read_request_body(&mut stream, &head).await;
                log.lock().unwrap().push((head, body));
                let _ = stream.write_all(wire.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), seen)
    }

    #[tokio::test]
    async fn get_json_decodes_and_asks_for_json() {
        let (base, seen) = server_replying("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 29\r\nConnection: close\r\n\r\n{\"name\":\"Ada\",\"age\":36,\"x\":1}");
        let user: User = Client::new().get_json(&format!("{base}/users/1")).await.expect("ответ 200 с верным JSON");
        assert_eq!(user, User { name: "Ada".into(), age: 36 });
        let head = seen.lock().unwrap()[0].0.to_ascii_lowercase();
        assert!(head.starts_with("get /users/1 http/1.1\r\n"), "{head:?}");
        assert!(head.contains("accept: application/json\r\n"), "клиент должен сказать, что ждёт JSON: {head:?}");
        assert!(head.contains("connection: close\r\n"), "у этого клиента на каждый запрос своё соединение, и сервер должен об этом знать: {head:?}");
    }

    #[tokio::test]
    async fn post_json_sends_a_correct_body() {
        let (base, seen) = server_replying("HTTP/1.1 201 Created\r\nContent-Length: 29\r\n\r\n{\"name\":\"Bob\",\"age\":41,\"x\":2}");
        let sent = serde_json::json!({ "name": "Bob", "tags": ["a", "б"] });
        let user: User = Client::new().post_json(&format!("{base}/users"), &sent).await.expect("201 это тоже успех");
        assert_eq!(user.name, "Bob");
        let (head, body) = seen.lock().unwrap()[0].clone();
        let head = head.to_ascii_lowercase();
        assert!(head.starts_with("post /users http/1.1\r\n"), "{head:?}");
        assert!(head.contains("content-type: application/json\r\n"), "{head:?}");
        assert!(head.contains(&format!("content-length: {}\r\n", body.len())), "длина в заголовке должна совпадать с телом: {head:?}");
        let value: serde_json::Value = serde_json::from_slice(&body).expect("тело запроса должно быть JSON");
        assert_eq!(value, sent);
    }

    #[tokio::test]
    async fn bad_status_is_not_a_decode_error() {
        let (base, _) = server_replying("HTTP/1.1 404 Not Found\r\nContent-Length: 12\r\n\r\nno such user");
        let err = Client::new().get_json::<User>(&format!("{base}/users/9")).await.unwrap_err();
        match err {
            ClientError::Status { code, body } => assert_eq!((code, body.as_str()), (404, "no such user")),
            other => panic!("404 это ошибка статуса с телом, а не {other:?}: тело ответа об ошибке не обязано быть JSON"),
        }
        let (base, _) = server_replying("HTTP/1.1 302 Found\r\nLocation: /x\r\nContent-Length: 0\r\n\r\n");
        let err = Client::new().get_json::<User>(&format!("{base}/")).await.unwrap_err();
        assert!(matches!(err, ClientError::Status { code: 302, .. }), "этот клиент редиректы не ходит, 3xx это не успех: {err:?}");
    }

    #[tokio::test]
    async fn garbage_json_is_a_decode_error() {
        let (base, _) = server_replying("HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\n<html>");
        let err = Client::new().get_json::<User>(&format!("{base}/")).await.unwrap_err();
        assert!(matches!(err, ClientError::Decode(_)), "200 с мусором это ошибка разбора, а не транспорта: {err:?}");
        let (base, _) = server_replying("HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\n{\"name\":\"only\"}");
        let err = Client::new().get_json::<User>(&format!("{base}/")).await.unwrap_err();
        assert!(matches!(err, ClientError::Decode(_)), "JSON верный, но не той формы: {err:?}");
    }

    #[tokio::test]
    async fn chunked_answers_work_too() {
        let (base, _) = server_replying("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\n{\"name\":\"Cy\",\"ag\r\n9\r\ne\":7}    \r\n0\r\n\r\n");
        let user: User = Client::new().get_json(&format!("{base}/")).await.expect("chunked-ответ собирается в целое тело");
        assert_eq!(user, User { name: "Cy".into(), age: 7 });
    }

    #[tokio::test]
    async fn failures_have_their_own_kinds() {
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        let err = Client::new().get_json::<User>(&format!("http://{closed}/")).await.unwrap_err();
        assert!(matches!(err, ClientError::Transport(_)), "порт никто не слушает: это транспорт, а не статус: {err:?}");
        let err = Client::new().get_json::<User>("https://example.com/").await.unwrap_err();
        assert!(matches!(err, ClientError::Url(UrlError::Scheme)), "{err:?}");
        let (addr, _) = spawn_server(|_, mut stream| async move {
            let _ = read_request_head(&mut stream).await;
        });
        let err = Client::new().get_json::<User>(&format!("http://{addr}/")).await.unwrap_err();
        assert!(matches!(err, ClientError::Transport(_)), "сервер закрыл соединение, ничего не ответив: {err:?}");
    }
}
