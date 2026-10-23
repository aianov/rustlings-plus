#![allow(dead_code)]
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

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

#[derive(Debug, Clone, Copy)]
struct Timeouts {
    connect: Duration,
    total: Duration,
}

#[derive(Debug)]
enum FetchError {
    ConnectTimeout,
    TotalTimeout,
    Connect(io::Error),
    Io(io::Error),
}

async fn fetch<S, C, Fut>(connect: C, url: &Url, timeouts: Timeouts) -> Result<Response, FetchError>
where
    S: AsyncRead + AsyncWrite + Unpin,
    C: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<S>>,
{
    let work = async {
        let mut stream = match timeout(timeouts.connect, connect(url.host.clone(), url.port)).await {
            Err(_) => return Err(FetchError::ConnectTimeout),
            Ok(Err(e)) => return Err(FetchError::Connect(e)),
            Ok(Ok(stream)) => stream,
        };
        let wire = build_request("GET", url, &[], None).map_err(|e| FetchError::Io(io::Error::new(io::ErrorKind::InvalidInput, format!("{e:?}"))))?;
        stream.write_all(&wire).await.map_err(FetchError::Io)?;
        let mut buf = Vec::new();
        read_response(&mut stream, &mut buf, "GET", 1 << 20).await.map_err(FetchError::Io)
    };
    timeout(timeouts.total, work).await.unwrap_or(Err(FetchError::TotalTimeout))
}

#[tokio::main]
async fn main() {
    let url = parse_url("http://127.0.0.1:1/").unwrap();
    let timeouts = Timeouts { connect: Duration::from_secs(1), total: Duration::from_secs(3) };
    let connect = |host: String, port: u16| async move { tokio::net::TcpStream::connect((host.as_str(), port)).await };
    println!("{:?}", fetch(connect, &url, timeouts).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{DuplexStream, duplex};
    use tokio::time::{Instant, sleep};

    const HEAD: &str = "HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n";

    fn url() -> Url {
        parse_url("http://example.test/").unwrap()
    }

    async fn drip(mut server: DuplexStream, first_after: Duration, every: Duration, bytes: usize) {
        sleep(first_after).await;
        let _ = server.write_all(HEAD.as_bytes()).await;
        for _ in 0..bytes {
            sleep(every).await;
            if server.write_all(b"x").await.is_err() {
                return;
            }
        }
        sleep(Duration::from_secs(3600)).await;
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[tokio::test(start_paused = true)]
    async fn connect_has_its_own_deadline() {
        let started = Instant::now();
        let connect = |_: String, _: u16| async { std::future::pending::<io::Result<DuplexStream>>().await };
        let result = fetch(connect, &url(), Timeouts { connect: secs(2), total: secs(30) }).await;
        assert!(matches!(result, Err(FetchError::ConnectTimeout)), "подключение зависло: ждали ConnectTimeout, получили {result:?}");
        let spent = started.elapsed();
        assert!(spent >= secs(2) && spent < secs(3), "ConnectTimeout должен сработать через 2 с, а не через {spent:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn total_deadline_beats_a_dripping_server() {
        let started = Instant::now();
        let connect = |_: String, _: u16| async {
            let (client, server) = duplex(65536);
            tokio::spawn(drip(server, Duration::ZERO, secs(4), 20));
            Ok::<_, io::Error>(client)
        };
        let result = fetch(connect, &url(), Timeouts { connect: secs(5), total: secs(10) }).await;
        assert!(matches!(result, Err(FetchError::TotalTimeout)), "сервер капает по байту раз в 4 с: нужен общий срок на запрос, получили {result:?}");
        let spent = started.elapsed();
        assert!(spent >= secs(10) && spent < secs(11), "общий срок 10 с, прошло {spent:?}: таймаут на каждое чтение такую капель не ловит");
    }

    #[tokio::test(start_paused = true)]
    async fn connect_time_counts_against_the_total() {
        let started = Instant::now();
        let connect = |_: String, _: u16| async {
            sleep(secs(8)).await;
            let (client, server) = duplex(65536);
            tokio::spawn(drip(server, secs(4), secs(1), 20));
            Ok::<_, io::Error>(client)
        };
        let result = fetch(connect, &url(), Timeouts { connect: secs(9), total: secs(10) }).await;
        assert!(matches!(result, Err(FetchError::TotalTimeout)), "8 с подключались и ещё 4 с ждали ответа, а общий срок 10 с: {result:?}");
        let spent = started.elapsed();
        assert!(spent >= secs(10) && spent < secs(11), "срок считается от начала, прошло {spent:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn quick_answer_is_fine() {
        let started = Instant::now();
        let connect = |_: String, _: u16| async {
            sleep(secs(1)).await;
            let (client, mut server) = duplex(65536);
            tokio::spawn(async move {
                let _ = server.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi").await;
                sleep(secs(3600)).await;
            });
            Ok::<_, io::Error>(client)
        };
        let response = fetch(connect, &url(), Timeouts { connect: secs(2), total: secs(30) }).await.expect("всё успевает");
        assert_eq!((response.status, response.text()), (200, "hi".to_string()));
        assert!(started.elapsed() < secs(2), "успешный запрос не должен ждать срока");
    }

    #[tokio::test(start_paused = true)]
    async fn connection_errors_are_not_timeouts() {
        let connect = |_: String, _: u16| async { Err::<DuplexStream, _>(io::Error::from(io::ErrorKind::ConnectionRefused)) };
        let result = fetch(connect, &url(), Timeouts { connect: secs(2), total: secs(30) }).await;
        match result {
            Err(FetchError::Connect(e)) => assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused),
            other => panic!("отказ в подключении это Connect(ошибка), а не таймаут: {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn broken_answer_is_io() {
        let connect = |_: String, _: u16| async {
            let (client, mut server) = duplex(65536);
            tokio::spawn(async move {
                let _ = server.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\nshort").await;
            });
            Ok::<_, io::Error>(client)
        };
        let result = fetch(connect, &url(), Timeouts { connect: secs(2), total: secs(30) }).await;
        assert!(matches!(result, Err(FetchError::Io(_))), "сервер закрыл соединение до конца тела: {result:?}");
    }
}
