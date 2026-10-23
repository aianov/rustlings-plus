// Клиент вправе слать запросы подряд, не дожидаясь ответов (конвейер): три запроса могут прийти одним
// сегментом, а четвёртый на полуслове. Цикл соединения готов, а read_request умеет только голову: тело
// запроса (POST с Content-Length) он принимает за начало следующего запроса, и соединение путается.
// TODO: Напиши read_request: верни один запрос вместе с телом, а всё после него оставь в буфере для следующего.
// Ok(None) при закрытии между запросами, UnexpectedEof посреди запроса, InvalidData на мусор.
// Тело читается по Content-Length и может содержать что угодно.

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

async fn read_request<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>) -> io::Result<Option<Request>> {
    todo!()
}

async fn connection<S, H>(mut stream: S, app: H) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: Fn(&Request) -> Response,
{
    let mut buf = Vec::new();
    while let Some(request) = read_request(&mut stream, &mut buf).await? {
        let close = request.header("connection").is_some_and(|v| v.eq_ignore_ascii_case("close"));
        let mut response = app(&request);
        if close {
            response = response.header("Connection", "close");
        }
        write_response(&mut stream, &response).await?;
        if close {
            break;
        }
    }
    stream.shutdown().await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (mut client, server) = tokio::io::duplex(1024);
    tokio::spawn(connection(server, |r| Response::new(200, &r.path)));
    client.write_all(b"GET /a HTTP/1.1\r\nHost: t\r\n\r\nGET /b HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").await?;
    let mut out = String::new();
    client.read_to_string(&mut out).await?;
    println!("{out}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn echo(request: &Request) -> Response {
        Response::new(200, &format!("{} {} {}", request.method, request.path, String::from_utf8_lossy(&request.body)))
    }

    async fn run(chunks: &[&[u8]], close_last: bool) -> Vec<String> {
        let (mut client, server) = duplex(64);
        let handle = tokio::spawn(connection(server, echo));
        let writer = {
            let chunks: Vec<Vec<u8>> = chunks.iter().map(|c| c.to_vec()).collect();
            async move {
                for chunk in chunks {
                    client.write_all(&chunk).await.unwrap();
                    tokio::task::yield_now().await;
                }
                if close_last {
                    client.write_all(b"GET /end HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").await.unwrap();
                }
                let mut buf = Vec::new();
                let mut out = Vec::new();
                while let Some(r) = timeout(Duration::from_secs(2), reply(&mut client, &mut buf)).await.expect("ответа не дождались") {
                    out.push(r.text());
                }
                out
            }
        };
        let out = writer.await;
        let _ = timeout(Duration::from_secs(2), handle).await.expect("connection завис");
        out
    }

    #[tokio::test]
    async fn three_requests_in_one_write() {
        let all: &[u8] = b"GET /1 HTTP/1.1\r\nHost: t\r\n\r\nPOST /2 HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhelloGET /3 HTTP/1.1\r\nHost: t\r\n\r\n";
        let out = run(&[all], true).await;
        assert_eq!(out, ["GET /1 ", "POST /2 hello", "GET /3 ", "GET /end "], "каждый из запросов в одном read должен быть обработан по порядку");
    }

    #[tokio::test]
    async fn requests_cut_at_every_possible_place() {
        let all: &[u8] = b"POST /a HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\n\r\nabcdGET /b HTTP/1.1\r\nHost: t\r\n\r\n";
        for cut in 1..all.len() {
            let out = run(&[&all[..cut], &all[cut..]], true).await;
            assert_eq!(out, ["POST /a abcd", "GET /b ", "GET /end "], "запрос, разрезанный на {cut}-м байте, должен собраться");
        }
    }

    #[tokio::test]
    async fn body_may_contain_a_fake_request() {
        let body = "GET /evil HTTP/1.1\r\nHost: t\r\n\r\n";
        let raw = format!("POST /a HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n{body}GET /b HTTP/1.1\r\nHost: t\r\n\r\n", body.len());
        let out = run(&[raw.as_bytes()], true).await;
        assert_eq!(out.len(), 3, "тело, похожее на запрос, остаётся телом: {out:?}");
        assert_eq!(out[1], "GET /b ");
    }

    #[tokio::test]
    async fn eof_cases() {
        let (mut client, server) = duplex(64);
        let handle = tokio::spawn(connection(server, echo));
        client.write_all(b"GET /a HTTP/1.1\r\nHo").await.unwrap();
        drop(client);
        let result = timeout(Duration::from_secs(2), handle).await.unwrap().unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof, "клиент ушёл посреди запроса");

        let (mut client, server) = duplex(64);
        let handle = tokio::spawn(connection(server, echo));
        client.write_all(b"BROKEN\r\n\r\n").await.unwrap();
        let result = timeout(Duration::from_secs(2), handle).await.unwrap().unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData, "мусор вместо запроса");
    }
}
