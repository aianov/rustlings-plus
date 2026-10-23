// curl при загрузке большого файла сначала шлёт одну голову с "Expect: 100-continue" и ждёт ответа:
// «давай тело» (100 Continue) или отказ. Если сервер молчит, curl терпит секунду и шлёт тело всё
// равно: каждая загрузка тормозит на секунду. Голова и тело читаются готовыми read_head и read_body.
// TODO: Допиши connection: слишком большая длина это 413 без всякого 100, другое Expect это 417,
// подходящий 100-continue получает "HTTP/1.1 100 Continue" до чтения тела (если тело уже пришло, не нужен).
// После успеха ответь 200 с текстом "got N bytes".

use std::time::Duration;
use tokio::time::timeout;

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

async fn read_head<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>) -> io::Result<Option<Request>> {
    loop {
        if let Some(end) = find_head_end(buf) {
            let head = String::from_utf8_lossy(&buf[..end]).into_owned();
            let request = parse_head(&head).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad request"))?;
            buf.drain(..end);
            return Ok(Some(request));
        }
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return if buf.is_empty() { Ok(None) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn read_body<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>, len: usize) -> io::Result<Vec<u8>> {
    while buf.len() < len {
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf.drain(..len).collect())
}

async fn connection<S>(mut stream: S, max_body: usize) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = connection(stream, 1 << 20).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

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

    async fn next(client: &mut DuplexStream, buf: &mut Vec<u8>) -> Reply {
        timeout(Duration::from_secs(5), reply(client, buf)).await.expect("ответа нет").expect("соединение закрыто")
    }

    #[tokio::test(start_paused = true)]
    async fn polite_client_gets_100_and_then_sends_the_body() {
        let (mut client, server) = duplex(4096);
        tokio::spawn(connection(server, 1000));
        let started = Instant::now();
        client.write_all(b"POST /up HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\nExpect: 100-continue\r\n\r\n").await.unwrap();
        let mut buf = Vec::new();
        let waited = timeout(Duration::from_secs(1), reply(&mut client, &mut buf)).await;
        let interim = waited.expect("curl прождал бы секунду: сервер не прислал 100 Continue").expect("ответ");
        assert_eq!(interim.status, 100);
        assert!(started.elapsed() < Duration::from_millis(100), "100 Continue пришёл через {:?}", started.elapsed());
        client.write_all(b"hello").await.unwrap();
        let done = next(&mut client, &mut buf).await;
        assert_eq!((done.status, done.text()), (200, "got 5 bytes".to_string()));
    }

    #[tokio::test(start_paused = true)]
    async fn too_large_is_refused_without_100() {
        let (mut client, server) = duplex(4096);
        tokio::spawn(connection(server, 1000));
        client.write_all(b"POST /up HTTP/1.1\r\nHost: t\r\nContent-Length: 5000000\r\nExpect: 100-continue\r\n\r\n").await.unwrap();
        let mut buf = Vec::new();
        let first = next(&mut client, &mut buf).await;
        assert_eq!(first.status, 413, "отказывать надо сразу: 100 Continue приглашает слать 5 МБ, которые не примут");
        assert_eq!(first.header("connection"), Some("close"));
    }

    #[tokio::test(start_paused = true)]
    async fn unknown_expectation_is_417() {
        let (mut client, server) = duplex(4096);
        tokio::spawn(connection(server, 1000));
        client.write_all(b"POST /up HTTP/1.1\r\nHost: t\r\nContent-Length: 1\r\nExpect: magic\r\n\r\n").await.unwrap();
        let mut buf = Vec::new();
        assert_eq!(next(&mut client, &mut buf).await.status, 417);
    }

    #[tokio::test(start_paused = true)]
    async fn no_interim_when_the_body_is_already_here() {
        let (mut client, server) = duplex(4096);
        tokio::spawn(connection(server, 1000));
        client.write_all(b"POST /up HTTP/1.1\r\nHost: t\r\nContent-Length: 3\r\nExpect: 100-continue\r\n\r\nabc").await.unwrap();
        let mut buf = Vec::new();
        let first = next(&mut client, &mut buf).await;
        assert_eq!((first.status, first.text()), (200, "got 3 bytes".to_string()), "клиент не стал ждать, тело пришло вместе с головой: слать 100 поздно");
    }

    #[tokio::test(start_paused = true)]
    async fn ordinary_requests_are_not_affected() {
        let (mut client, server) = duplex(4096);
        tokio::spawn(connection(server, 1000));
        client.write_all(b"POST /up HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\n\r\nabcd").await.unwrap();
        let mut buf = Vec::new();
        let first = next(&mut client, &mut buf).await;
        assert_eq!((first.status, first.text()), (200, "got 4 bytes".to_string()));
    }
}
