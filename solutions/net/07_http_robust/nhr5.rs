use tokio::io::duplex;

use std::io;
#[allow(unused_imports)]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq)]
struct Request {
    method: String,
    path: String,
    minor: u8,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Response {
    fn new(status: u16, body: &str) -> Response {
        Response { status, headers: Vec::new(), body: body.as_bytes().to_vec() }
    }

    fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

#[allow(dead_code)]
fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        417 => "Expectation Failed",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        _ => "",
    }
}

#[allow(dead_code)]
async fn write_response<W: AsyncWrite + Unpin>(w: &mut W, response: &Response) -> io::Result<()> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status)).into_bytes();
    for (name, value) in &response.headers {
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", response.body.len()).as_bytes());
    out.extend_from_slice(&response.body);
    w.write_all(&out).await
}

#[allow(dead_code)]
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

#[allow(dead_code)]
fn parse_head(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut parts = lines.next()?.split(' ');
    let (method, path, version) = (parts.next()?, parts.next()?, parts.next()?);
    let minor = match version {
        "HTTP/1.0" => 0,
        "HTTP/1.1" => 1,
        _ => return None,
    };
    if parts.next().is_some() || !path.starts_with('/') {
        return None;
    }
    let mut headers = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':')?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    Some(Request { method: method.to_string(), path: path.to_string(), minor, headers, body: Vec::new() })
}

#[allow(dead_code)]
fn content_length(request: &Request) -> usize {
    request.header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0)
}

struct Limits {
    max_head: usize,
    max_body: usize,
}

enum Outcome {
    Request(Request),
    Closed,
    Reject(u16),
}

async fn read_request<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>, limits: &Limits) -> io::Result<Outcome> {
    loop {
        match find_head_end(buf) {
            Some(end) if end > limits.max_head => return Ok(Outcome::Reject(431)),
            None if buf.len() >= limits.max_head => return Ok(Outcome::Reject(431)),
            Some(end) => {
                let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                let Some(mut request) = parse_head(&head) else {
                    return Ok(Outcome::Reject(400));
                };
                let length = match request.header("content-length") {
                    None => 0,
                    Some(raw) if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) => match raw.parse::<usize>() {
                        Ok(n) => n,
                        Err(_) => return Ok(Outcome::Reject(413)),
                    },
                    Some(_) => return Ok(Outcome::Reject(400)),
                };
                if length > limits.max_body {
                    return Ok(Outcome::Reject(413));
                }
                if buf.len() >= end + length {
                    request.body = buf[end..end + length].to_vec();
                    buf.drain(..end + length);
                    return Ok(Outcome::Request(request));
                }
            }
            None => {}
        }
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return if buf.is_empty() { Ok(Outcome::Closed) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn connection<S>(mut stream: S, limits: Limits) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    match read_request(&mut stream, &mut buf, &limits).await? {
        Outcome::Request(request) => {
            let reply = Response::new(200, &format!("{} bytes", request.body.len()));
            write_response(&mut stream, &reply).await?;
        }
        Outcome::Closed => {}
        Outcome::Reject(status) => {
            let reply = Response::new(status, "rejected").header("Connection", "close");
            write_response(&mut stream, &reply).await?;
        }
    }
    stream.shutdown().await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (mut client, server) = duplex(1024);
    tokio::spawn(connection(server, Limits { max_head: 1024, max_body: 1024 }));
    client.write_all(b"POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 99999999\r\n\r\n").await?;
    let mut out = String::new();
    client.read_to_string(&mut out).await?;
    println!("{out}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::timeout;

    use tokio::io::{DuplexStream, duplex};

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Reply {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    #[allow(dead_code)]
    impl Reply {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&self.body).into_owned()
        }
    }

    async fn reply(client: &mut DuplexStream, buf: &mut Vec<u8>) -> Option<Reply> {
        loop {
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4) {
                let head = String::from_utf8(buf[..end].to_vec()).unwrap();
                let mut lines = head.split("\r\n");
                let status: u16 = lines.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
                let headers: Vec<(String, String)> = lines
                    .filter(|l| !l.is_empty())
                    .map(|l| {
                        let (n, v) = l.split_once(':').unwrap();
                        (n.trim().to_string(), v.trim().to_string())
                    })
                    .collect();
                let len: usize = headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.parse().unwrap())
                    .unwrap_or(0);
                if status == 100 || buf.len() >= end + len {
                    let total = if status == 100 { end } else { end + len };
                    let body = buf[end..total].to_vec();
                    buf.drain(..total);
                    return Some(Reply { status, headers, body });
                }
            }
            let mut chunk = [0u8; 2048];
            let n = client.read(&mut chunk).await.unwrap_or(0);
            if n == 0 {
                assert!(buf.is_empty(), "соединение закрыто посреди ответа: {:?}", String::from_utf8_lossy(buf));
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn limits() -> Limits {
        Limits { max_head: 1024, max_body: 100 }
    }

    async fn status_of(raw: Vec<u8>) -> u16 {
        let (mut client, server) = duplex(256);
        tokio::spawn(connection(server, limits()));
        tokio::spawn(async move {
            let _ = client.write_all(&raw).await;
            let mut buf = Vec::new();
            reply(&mut client, &mut buf).await
        })
        .await
        .unwrap()
        .map(|r| r.status)
        .unwrap_or(0)
    }

    #[tokio::test]
    async fn endless_head_is_cut_off_early() {
        let (mut client, server) = duplex(256);
        let handle = tokio::spawn(connection(server, limits()));
        let written = Arc::new(AtomicUsize::new(0));
        let counter = written.clone();
        let writer = tokio::spawn(async move {
            client.write_all(b"GET / HTTP/1.1\r\nHost: t\r\n").await.ok();
            loop {
                if client.write_all(b"X-Filler: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n").await.is_err() {
                    break;
                }
                counter.fetch_add(41, Ordering::SeqCst);
                if counter.load(Ordering::SeqCst) > 5_000_000 {
                    break;
                }
            }
            let mut buf = Vec::new();
            reply(&mut client, &mut buf).await
        });
        let result = timeout(Duration::from_secs(5), writer).await.expect("клиент так и не получил ответ").unwrap();
        assert_eq!(result.expect("ответ").status, 431, "голова без конца это 431");
        let sent = written.load(Ordering::SeqCst);
        assert!(sent < 10_000, "сервер дочитывал голову до {sent} байт, а лимит 1024: останавливаться надо сразу");
        let _ = handle.await;
    }

    #[tokio::test]
    async fn big_complete_head_is_431_too() {
        let mut raw = b"GET / HTTP/1.1\r\nHost: t\r\n".to_vec();
        raw.extend_from_slice(format!("X-Big: {}\r\n\r\n", "a".repeat(2000)).as_bytes());
        assert_eq!(status_of(raw).await, 431, "голова пришла целиком, но не помещается");
    }

    #[tokio::test]
    async fn declared_length_is_checked_before_the_body() {
        let status = timeout(Duration::from_secs(2), status_of(b"POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 10000000\r\n\r\n".to_vec()))
            .await
            .expect("сервер ждёт тело, которое всё равно не примет: нужно ответить сразу");
        assert_eq!(status, 413);
        let status = timeout(Duration::from_secs(2), status_of(b"POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 99999999999999999999999\r\n\r\n".to_vec()))
            .await
            .expect("не дождались ответа");
        assert_eq!(status, 413, "число, не влезающее даже в usize, это тем более слишком много");
    }

    #[tokio::test]
    async fn garbage_length_is_400() {
        for value in ["abc", "-5", "1 2", "", "+7", "0x10"] {
            let raw = format!("POST / HTTP/1.1\r\nHost: t\r\nContent-Length: {value}\r\n\r\nxxxx");
            assert_eq!(status_of(raw.into_bytes()).await, 400, "Content-Length: {value:?} не число");
        }
    }

    #[tokio::test]
    async fn limits_are_inclusive() {
        let prefix = "GET / HTTP/1.1\r\nHost: t\r\nX-Pad: ";
        let pad = 1024 - prefix.len() - 4;
        let exact = format!("{prefix}{}\r\n\r\n", "a".repeat(pad));
        assert_eq!(exact.len(), 1024);
        assert_eq!(status_of(exact.into_bytes()).await, 200, "голова ровно max_head проходит");
        let over = format!("{prefix}{}\r\n\r\n", "a".repeat(pad + 1));
        assert_eq!(status_of(over.into_bytes()).await, 431, "на байт больше уже нельзя");
        let body = "b".repeat(100);
        let raw = format!("POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 100\r\n\r\n{body}");
        assert_eq!(status_of(raw.into_bytes()).await, 200, "тело ровно max_body проходит");
        let raw = format!("POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 101\r\n\r\n{body}b");
        assert_eq!(status_of(raw.into_bytes()).await, 413);
    }
}
