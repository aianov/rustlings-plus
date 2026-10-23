// Защита от медленных клиентов уже есть: read_head ждёт каждый read не дольше patience секунд, иначе 408.
// Но «злоумышленник» присылает по одному байту раз в четыре секунды. Каждый read укладывается в
// patience, ни один лимит размера не нарушен, а соединение занято навсегда: тысяча таких клиентов
// и сервер больше никого не принимает. Тест это показывает.
// TODO: Почини read_head: на всю голову запроса отводится deadline, считая с первого байта. Дольше
// это Err(Timeout) (и 408 клиенту). Молчание до первого байта дольше patience не наказание, а простой: Ok(None).
use std::time::Duration;
use tokio::time::{Instant, timeout};

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

#[derive(Debug, PartialEq)]
enum HeadError {
    Timeout,
    Io(io::ErrorKind),
}

async fn read_head<S: AsyncRead + Unpin>(
    stream: &mut S,
    buf: &mut Vec<u8>,
    patience: Duration,
    deadline: Duration,
) -> Result<Option<usize>, HeadError> {
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = find_head_end(buf) {
            return Ok(Some(end));
        }
        let read = timeout(patience, stream.read(&mut chunk)).await.map_err(|_| HeadError::Timeout)?;
        let n = read.map_err(|e| HeadError::Io(e.kind()))?;
        if n == 0 {
            return if buf.is_empty() { Ok(None) } else { Err(HeadError::Io(io::ErrorKind::UnexpectedEof)) };
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn connection<S>(mut stream: S, patience: Duration, deadline: Duration) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    match read_head(&mut stream, &mut buf, patience, deadline).await {
        Ok(Some(_)) => write_response(&mut stream, &Response::new(200, "hello")).await?,
        Ok(None) => {}
        Err(HeadError::Timeout) => {
            write_response(&mut stream, &Response::new(408, "request timeout").header("Connection", "close")).await?
        }
        Err(HeadError::Io(kind)) => return Err(kind.into()),
    }
    stream.shutdown().await
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = connection(stream, Duration::from_secs(30), Duration::from_secs(10)).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

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

    const PATIENCE: Duration = Duration::from_secs(5);
    const DEADLINE: Duration = Duration::from_secs(10);

    async fn first_reply(client: &mut DuplexStream) -> Option<Reply> {
        let mut buf = Vec::new();
        timeout(Duration::from_secs(120), reply(client, &mut buf)).await.expect("сервер так и не ответил и не закрыл соединение")
    }

    #[tokio::test(start_paused = true)]
    async fn dripping_client_gets_408_by_the_deadline() {
        let (mut client, server) = duplex(1024);
        tokio::spawn(connection(server, PATIENCE, DEADLINE));
        let started = Instant::now();
        let drip = tokio::spawn(async move {
            client.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
            for _ in 0..100 {
                sleep(Duration::from_secs(4)).await;
                if client.write_all(b"X-Drip: a\r\n").await.is_err() {
                    break;
                }
            }
            first_reply(&mut client).await
        });
        let reply = timeout(Duration::from_secs(600), drip).await.expect("тест завис").unwrap();
        let reply = reply.expect("перед закрытием ждали 408");
        assert_eq!(reply.status, 408);
        let took = started.elapsed();
        assert!(took >= DEADLINE && took < DEADLINE + Duration::from_secs(6), "408 должен прийти около дедлайна ({DEADLINE:?}), а пришёл через {took:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_mid_head_does_not_wait_for_patience() {
        let (mut client, server) = duplex(1024);
        let started = Instant::now();
        tokio::spawn(connection(server, Duration::from_secs(60), Duration::from_secs(5)));
        client.write_all(b"GET / HTTP/1.1\r\nHo").await.unwrap();
        let reply = first_reply(&mut client).await.expect("ответ 408");
        assert_eq!(reply.status, 408);
        assert!(started.elapsed() < Duration::from_secs(8), "дедлайн на голову 5 секунд, а ждали {:?}", started.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn slow_but_honest_client_is_served() {
        let (mut client, server) = duplex(1024);
        tokio::spawn(connection(server, PATIENCE, DEADLINE));
        for piece in ["GET / HT", "TP/1.1\r\n", "Host: t\r\n", "\r\n"] {
            client.write_all(piece.as_bytes()).await.unwrap();
            sleep(Duration::from_secs(2)).await;
        }
        let reply = first_reply(&mut client).await.expect("ответ");
        assert_eq!((reply.status, reply.text()), (200, "hello".to_string()), "8 секунд на голову укладываются в дедлайн 10");
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_starts_at_the_first_byte_not_at_connect() {
        let (mut client, server) = duplex(1024);
        tokio::spawn(connection(server, Duration::from_secs(30), Duration::from_secs(5)));
        sleep(Duration::from_secs(8)).await;
        client.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        sleep(Duration::from_secs(1)).await;
        client.write_all(b"Host: t\r\n\r\n").await.unwrap();
        let reply = first_reply(&mut client).await.expect("ответ");
        assert_eq!(reply.status, 200, "до первого байта клиент просто думал, это простой, а не медленная голова");
    }

    #[tokio::test(start_paused = true)]
    async fn silence_before_the_first_byte_closes_quietly() {
        let (mut client, server) = duplex(1024);
        let handle = tokio::spawn(connection(server, PATIENCE, DEADLINE));
        let mut rest = Vec::new();
        timeout(Duration::from_secs(60), client.read_to_end(&mut rest)).await.expect("соединение не закрылось").unwrap();
        assert!(rest.is_empty(), "тихий клиент не получает 408: {rest:?}");
        assert!(handle.await.unwrap().is_ok());
    }
}
