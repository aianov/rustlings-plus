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

async fn read_request<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>) -> io::Result<Option<Request>> {
    loop {
        if let Some(end) = find_head_end(buf) {
            let head = String::from_utf8_lossy(&buf[..end]).into_owned();
            let mut request = parse_head(&head).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad request"))?;
            let total = end + content_length(&request);
            if buf.len() >= total {
                request.body = buf[end..total].to_vec();
                buf.drain(..total);
                return Ok(Some(request));
            }
        }
        let mut chunk = [0u8; 1024];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return if buf.is_empty() { Ok(None) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn connection<S, H>(mut stream: S, idle: Duration, app: H) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: Fn(&Request) -> Response,
{
    let mut buf = Vec::new();
    loop {
        if buf.is_empty() {
            let mut first = [0u8; 1024];
            let Ok(read) = timeout(idle, stream.read(&mut first)).await else {
                return stream.shutdown().await;
            };
            let n = read?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&first[..n]);
        }
        let Some(request) = read_request(&mut stream, &mut buf).await? else {
            return Ok(());
        };
        let response = app(&request);
        write_response(&mut stream, &response).await?;
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = connection(stream, Duration::from_secs(30), |r| Response::new(200, &r.path)).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, sleep};

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

    const IDLE: Duration = Duration::from_secs(30);

    fn echo(request: &Request) -> Response {
        Response::new(200, &request.path)
    }

    async fn ask(client: &mut DuplexStream, buf: &mut Vec<u8>, path: &str) -> Reply {
        client.write_all(format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n").as_bytes()).await.unwrap();
        timeout(Duration::from_secs(5), reply(client, buf)).await.expect("нет ответа").expect("соединение закрыто")
    }

    #[tokio::test(start_paused = true)]
    async fn silent_connection_is_closed_quietly() {
        let (mut client, server) = duplex(1024);
        let handle = tokio::spawn(connection(server, IDLE, echo));
        let started = Instant::now();
        let mut rest = Vec::new();
        timeout(Duration::from_secs(120), client.read_to_end(&mut rest)).await.expect("сервер так и не закрыл молчащее соединение").unwrap();
        assert!(rest.is_empty(), "закрывать надо молча, без 408 и без чего-либо ещё: {rest:?}");
        let waited = started.elapsed();
        assert!(waited >= IDLE && waited < IDLE + Duration::from_secs(5), "закрылось через {waited:?}, ждали около {IDLE:?}");
        assert!(handle.await.unwrap().is_ok(), "простой это не ошибка");
    }

    #[tokio::test(start_paused = true)]
    async fn idle_timer_restarts_after_each_response() {
        let (mut client, server) = duplex(1024);
        let handle = tokio::spawn(connection(server, IDLE, echo));
        let mut buf = Vec::new();
        for i in 0..6 {
            let r = ask(&mut client, &mut buf, &format!("/{i}")).await;
            assert_eq!(r.text(), format!("/{i}"));
            sleep(Duration::from_secs(20)).await;
        }
        assert!(!handle.is_finished(), "запросы идут каждые 20 секунд, это не простой: соединение живёт");
        let r = ask(&mut client, &mut buf, "/last").await;
        assert_eq!(r.text(), "/last");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_request_is_not_idle_time() {
        let (mut client, server) = duplex(1024);
        let handle = tokio::spawn(connection(server, IDLE, echo));
        for piece in ["GET /slow HTTP/1.1\r\n", "Host: t\r\n", "X-A: 1\r\n", "\r\n"] {
            client.write_all(piece.as_bytes()).await.unwrap();
            sleep(Duration::from_secs(20)).await;
            assert!(!handle.is_finished(), "запрос уже начался: простоем считается только тишина до первого байта запроса");
        }
        let mut buf = Vec::new();
        let r = timeout(Duration::from_secs(5), reply(&mut client, &mut buf)).await.unwrap().expect("ответ");
        assert_eq!(r.text(), "/slow");
    }

    #[tokio::test(start_paused = true)]
    async fn closes_after_the_last_request_goes_quiet() {
        let (mut client, server) = duplex(1024);
        let handle = tokio::spawn(connection(server, IDLE, echo));
        let mut buf = Vec::new();
        ask(&mut client, &mut buf, "/one").await;
        let started = Instant::now();
        assert!(timeout(Duration::from_secs(120), reply(&mut client, &mut buf)).await.expect("не закрылось").is_none(), "лишних байт нет, соединение просто закрыто");
        assert!(started.elapsed() >= IDLE, "закрыли раньше времени: {:?}", started.elapsed());
        assert!(handle.await.unwrap().is_ok());
    }
}
