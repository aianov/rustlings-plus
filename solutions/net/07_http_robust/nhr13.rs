use std::pin::Pin;
use std::task::{Context, Poll};

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

async fn write_response<W: AsyncWrite + Unpin>(w: &mut W, response: &Response, head_only: bool) -> io::Result<()> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status)).into_bytes();
    for (name, value) in &response.headers {
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    let bodyless = response.status < 200 || response.status == 204 || response.status == 304;
    if !bodyless {
        out.extend_from_slice(format!("Content-Length: {}\r\n", response.body.len()).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    if !bodyless && !head_only {
        out.extend_from_slice(&response.body);
    }
    w.write_all(&out).await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (mut client, mut server) = tokio::io::duplex(4096);
    write_response(&mut server, &Response::new(200, "hello").header("X-Demo", "1"), false).await?;
    drop(server);
    let mut out = String::new();
    client.read_to_string(&mut out).await?;
    println!("{out}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting {
        calls: Vec<Vec<u8>>,
        max_per_call: usize,
    }

    impl AsyncWrite for Counting {
        fn poll_write(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            let n = buf.len().min(self.max_per_call);
            self.calls.push(buf[..n].to_vec());
            Poll::Ready(Ok(n))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl Counting {
        fn new(max_per_call: usize) -> Counting {
            Counting { calls: Vec::new(), max_per_call }
        }

        fn all(&self) -> String {
            String::from_utf8(self.calls.concat()).unwrap()
        }
    }

    #[tokio::test]
    async fn small_response_is_one_write() {
        let mut w = Counting::new(usize::MAX);
        write_response(&mut w, &Response::new(200, "hello").header("X-A", "1"), false).await.unwrap();
        assert_eq!(w.calls.len(), 1, "голова и тело ушли {} записями, а должна быть одна", w.calls.len());
        assert_eq!(w.all(), "HTTP/1.1 200 OK\r\nX-A: 1\r\nContent-Length: 5\r\n\r\nhello");
    }

    #[tokio::test]
    async fn first_write_carries_head_and_start_of_body() {
        let body = "x".repeat(5000);
        let mut w = Counting::new(1000);
        write_response(&mut w, &Response::new(200, &body), false).await.unwrap();
        assert_eq!(w.calls[0].len(), 1000, "первая запись должна заполнить доступное место: голова и начало тела вместе, а не голова отдельно");
        assert!(w.all().ends_with(&body));
        assert_eq!(w.calls.len(), 1 + (w.all().len() - 1) / 1000, "лишних записей быть не должно");
    }

    #[tokio::test]
    async fn head_requests_have_length_but_no_body() {
        let mut w = Counting::new(usize::MAX);
        write_response(&mut w, &Response::new(200, "hello"), true).await.unwrap();
        assert_eq!(w.calls.len(), 1);
        assert_eq!(w.all(), "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n", "на HEAD длина настоящая, а тела нет");
    }

    #[tokio::test]
    async fn bodyless_statuses_have_neither_length_nor_body() {
        let mut w = Counting::new(usize::MAX);
        write_response(&mut w, &Response::new(204, "ignored"), false).await.unwrap();
        assert_eq!(w.all(), "HTTP/1.1 204 No Content\r\n\r\n", "у 204 нет ни Content-Length, ни тела, даже если его передали");
        assert_eq!(w.calls.len(), 1);
    }

    #[tokio::test]
    async fn empty_body_is_one_write_too() {
        let mut w = Counting::new(usize::MAX);
        write_response(&mut w, &Response::new(204, ""), false).await.unwrap();
        assert_eq!(w.calls.len(), 1, "пустое тело не повод для второй записи");
    }
}
