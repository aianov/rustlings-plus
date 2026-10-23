// Пул держит соединения, а сервер закрывает простаивающие тихо и без предупреждения. Первый запрос после долгой
// паузы уходит в закрытый сокет и получает обрыв. Это не ошибка сервера, это гонка, и все клиенты с пулом о ней знают.
// Пользователь не должен видеть этот сбой, но повторять можно не всё: если запрос успел выполниться, его повтор
// сделает деньги дважды.
// TODO: Допиши is_idempotent и request. Протухшее соединение (exchange вернул Stale) из пула повторяем один раз на
// свежем, и только если метод идемпотентный. Свежее соединение и любые другие сбои не повторяем. Годные соединения
// возвращаются в пул.

#![allow(dead_code)]
use std::collections::HashMap;
use std::io;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;

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

type Key = (String, u16);

struct Pool<S> {
    max_idle: usize,
    ttl: Duration,
    idle: HashMap<Key, Vec<(Instant, S)>>,
}

impl<S> Pool<S> {
    fn new(max_idle: usize, ttl: Duration) -> Pool<S> {
        Pool { max_idle, ttl, idle: HashMap::new() }
    }

    fn take(&mut self, key: &Key) -> Option<S> {
        let list = self.idle.get_mut(key)?;
        let (now, ttl) = (Instant::now(), self.ttl);
        list.retain(|(since, _)| now.duration_since(*since) < ttl);
        list.pop().map(|(_, s)| s)
    }

    fn put(&mut self, key: Key, conn: S) {
        let list = self.idle.entry(key).or_default();
        list.push((Instant::now(), conn));
        while list.len() > self.max_idle {
            list.remove(0);
        }
    }

    fn idle(&self) -> usize {
        self.idle.values().map(|l| l.len()).sum()
    }
}

#[derive(Debug)]
enum Error {
    Url(UrlError),
    Request(RequestError),
    Io(io::Error),
}

#[derive(Debug)]
enum ExchangeError {
    Stale(io::Error),
    Failed(io::Error),
}

fn reset_like(kind: io::ErrorKind) -> bool {
    matches!(kind, io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted)
}

async fn exchange(stream: &mut TcpStream, wire: &[u8], method: &str) -> Result<Response, ExchangeError> {
    if let Err(e) = stream.write_all(wire).await {
        return Err(if reset_like(e.kind()) { ExchangeError::Stale(e) } else { ExchangeError::Failed(e) });
    }
    let mut buf = Vec::new();
    match fill(stream, &mut buf).await {
        Ok(0) => return Err(ExchangeError::Stale(io::ErrorKind::UnexpectedEof.into())),
        Ok(_) => {}
        Err(e) if reset_like(e.kind()) => return Err(ExchangeError::Stale(e)),
        Err(e) => return Err(ExchangeError::Failed(e)),
    }
    read_response(stream, &mut buf, method, 1 << 20).await.map_err(ExchangeError::Failed)
}

fn is_idempotent(method: &str) -> bool {
    let _ = method;
    todo!()
}

struct Client {
    pool: Mutex<Pool<TcpStream>>,
}

impl Client {
    fn new() -> Client {
        Client { pool: Mutex::new(Pool::new(4, Duration::from_secs(30))) }
    }

    async fn connect(&self, url: &Url) -> Result<TcpStream, Error> {
        TcpStream::connect((url.host.as_str(), url.port)).await.map_err(Error::Io)
    }

    async fn request(&self, method: &str, url: &str, body: Option<&[u8]>) -> Result<Response, Error> {
        let _ = (method, url, body);
        todo!()
    }

    fn idle(&self) -> usize {
        self.pool.lock().unwrap().idle()
    }
}

#[tokio::main]
async fn main() {
    let client = Client::new();
    println!("{:?}", client.request("GET", "http://127.0.0.1:1/", None).await.map(|r| r.status));
}

#[cfg(test)]
mod tests {
    use super::*;
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

    const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";

    async fn serve_loop(mut stream: TcpStream, seen: Arc<AtomicUsize>) {
        while let Some(head) = read_request_head(&mut stream).await {
            let _ = read_request_body(&mut stream, &head).await;
            seen.fetch_add(1, Ordering::SeqCst);
            if stream.write_all(OK).await.is_err() {
                return;
            }
        }
    }

    async fn one_then_close(mut stream: TcpStream, seen: Arc<AtomicUsize>) {
        if let Some(head) = read_request_head(&mut stream).await {
            let _ = read_request_body(&mut stream, &head).await;
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(OK).await;
        }
    }

    async fn pause() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn idempotent_requests_survive_an_idle_close() {
        for (method, body) in [("GET", None), ("HEAD", None), ("PUT", Some(b"v".as_slice())), ("DELETE", None), ("OPTIONS", None)] {
            let requests = Arc::new(AtomicUsize::new(0));
            let r = requests.clone();
            let (addr, accepted) = spawn_server(move |n, stream| {
                let r = r.clone();
                async move {
                    if n == 0 {
                        one_then_close(stream, r).await;
                    } else {
                        serve_loop(stream, r).await;
                    }
                }
            });
            let client = Client::new();
            let url = format!("http://{addr}/thing");
            assert_eq!(client.request(method, &url, body).await.unwrap().status, 200);
            pause().await;
            let second = client.request(method, &url, body).await.unwrap_or_else(|e| panic!("{method} на протухшем соединении нужно повторить на новом: {e:?}"));
            assert_eq!(second.status, 200);
            assert_eq!(accepted.load(Ordering::SeqCst), 2, "{method}: должно быть ровно одно новое соединение");
        }
    }

    #[tokio::test]
    async fn post_is_never_retried() {
        let requests = Arc::new(AtomicUsize::new(0));
        let r = requests.clone();
        let (addr, accepted) = spawn_server(move |n, stream| {
            let r = r.clone();
            async move {
                if n == 0 {
                    one_then_close(stream, r).await;
                } else {
                    serve_loop(stream, r).await;
                }
            }
        });
        let client = Client::new();
        let url = format!("http://{addr}/orders");
        assert_eq!(client.request("POST", &url, Some(b"{}")).await.unwrap().status, 200);
        pause().await;
        let second = client.request("POST", &url, Some(b"{}")).await;
        assert!(second.is_err(), "POST мог выполниться на сервере до обрыва, повторять его нельзя: {second:?}");
        assert_eq!(accepted.load(Ordering::SeqCst), 1, "новое соединение для POST открывать не надо");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        let third = client.request("POST", &url, Some(b"{}")).await.expect("а вот следующий POST пойдёт на свежем соединении: протухшее уже выброшено");
        assert_eq!(third.status, 200);
        assert_eq!(accepted.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fresh_connection_failure_is_not_retried() {
        let (addr, accepted) = spawn_server(|_, mut stream| async move {
            let _ = read_request_head(&mut stream).await;
        });
        let client = Client::new();
        let result = client.request("GET", &format!("http://{addr}/"), None).await;
        assert!(result.is_err(), "сервер закрывает, не ответив");
        assert_eq!(accepted.load(Ordering::SeqCst), 1, "свежее соединение подвело: повтор на новом ничего не докажет, это уже сбой сервера");
    }

    #[tokio::test]
    async fn retry_happens_only_once() {
        let requests = Arc::new(AtomicUsize::new(0));
        let r = requests.clone();
        let (addr, accepted) = spawn_server(move |n, mut stream| {
            let r = r.clone();
            async move {
                if n == 0 {
                    one_then_close(stream, r).await;
                } else {
                    let _ = read_request_head(&mut stream).await;
                }
            }
        });
        let client = Client::new();
        let url = format!("http://{addr}/");
        assert_eq!(client.request("GET", &url, None).await.unwrap().status, 200);
        pause().await;
        let second = client.request("GET", &url, None).await;
        assert!(second.is_err());
        assert_eq!(accepted.load(Ordering::SeqCst), 2, "одно протухшее и один повтор; ещё одно соединение значило бы бесконечный повтор");
    }

    #[tokio::test]
    async fn healthy_pool_connections_are_used_without_retry() {
        let requests = Arc::new(AtomicUsize::new(0));
        let r = requests.clone();
        let (addr, accepted) = spawn_server(move |_, stream| serve_loop(stream, r.clone()));
        let client = Client::new();
        for _ in 0..4 {
            assert_eq!(client.request("GET", &format!("http://{addr}/"), None).await.unwrap().status, 200);
        }
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
        assert_eq!(requests.load(Ordering::SeqCst), 4);
        assert_eq!(client.idle(), 1);
    }
}
