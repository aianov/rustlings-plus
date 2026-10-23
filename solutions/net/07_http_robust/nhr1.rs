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

async fn read_request<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>) -> io::Result<Option<Request>> {
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

fn wants_keep_alive(request: &Request) -> bool {
    let mut close = false;
    let mut keep = false;
    for (_, value) in request.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case("connection")) {
        for token in value.split(',') {
            let token = token.trim();
            close |= token.eq_ignore_ascii_case("close");
            keep |= token.eq_ignore_ascii_case("keep-alive");
        }
    }
    !close && (keep || request.minor == 1)
}

async fn connection<S, H>(mut stream: S, app: H) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    H: Fn(&Request) -> Response,
{
    let mut buf = Vec::new();
    while let Some(request) = read_request(&mut stream, &mut buf).await? {
        let keep = wants_keep_alive(&request);
        let mut response = app(&request);
        if !keep {
            response = response.header("Connection", "close");
        } else if request.minor == 0 {
            response = response.header("Connection", "keep-alive");
        }
        write_response(&mut stream, &response).await?;
        if !keep {
            break;
        }
    }
    stream.shutdown().await
}

fn app(request: &Request) -> Response {
    Response::new(200, &format!("you asked for {}", request.path))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = connection(stream, app).await;
        });
    }
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
        Response::new(200, &request.path)
    }

    fn spawn_server() -> (DuplexStream, tokio::task::JoinHandle<io::Result<()>>) {
        let (client, server) = duplex(4096);
        (client, tokio::spawn(connection(server, echo)))
    }

    async fn ask(client: &mut DuplexStream, buf: &mut Vec<u8>, raw: &str) -> Reply {
        client.write_all(raw.as_bytes()).await.unwrap();
        timeout(Duration::from_secs(2), reply(client, buf)).await.expect("сервер не ответил за 2 секунды").expect("сервер закрыл соединение, не ответив")
    }

    #[tokio::test]
    async fn serves_many_requests_on_one_connection() {
        let (mut client, server) = spawn_server();
        let mut buf = Vec::new();
        for path in ["/a", "/b", "/c"] {
            let r = ask(&mut client, &mut buf, &format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n")).await;
            assert_eq!(r.text(), path, "каждый запрос на том же соединении получает свой ответ");
            assert_ne!(r.header("connection"), Some("close"), "пока не закрываемся, закрытием не пугаем");
        }
        drop(client);
        let result = timeout(Duration::from_secs(2), server).await.expect("connection не завершился после ухода клиента").unwrap();
        assert!(result.is_ok(), "клиент ушёл между запросами: это нормальный конец, а не ошибка ({result:?})");
    }

    #[tokio::test]
    async fn connection_close_ends_the_conversation() {
        let (mut client, server) = spawn_server();
        let mut buf = Vec::new();
        let r = ask(&mut client, &mut buf, "GET /bye HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").await;
        assert_eq!(r.text(), "/bye");
        assert_eq!(r.header("connection"), Some("close"), "закрывая соединение, сервер обязан об этом сказать");
        let result = timeout(Duration::from_secs(2), server).await.expect("сервер должен сам закрыть соединение").unwrap();
        assert!(result.is_ok());
        let mut rest = Vec::new();
        timeout(Duration::from_secs(1), client.read_to_end(&mut rest)).await.unwrap().unwrap();
        assert!(rest.is_empty(), "после закрытия лишних байт быть не должно: {rest:?}");
    }

    #[tokio::test]
    async fn http10_closes_by_default_but_can_keep() {
        let (mut client, server) = spawn_server();
        let mut buf = Vec::new();
        let r = ask(&mut client, &mut buf, "GET /old HTTP/1.0\r\n\r\n").await;
        assert_eq!(r.header("connection"), Some("close"), "HTTP/1.0 без keep-alive закрывается");
        timeout(Duration::from_secs(2), server).await.expect("сервер должен закрыть соединение").unwrap().unwrap();

        let (mut client, _server) = spawn_server();
        let mut buf = Vec::new();
        let r = ask(&mut client, &mut buf, "GET /1 HTTP/1.0\r\nConnection: keep-alive\r\n\r\n").await;
        assert_eq!(r.header("connection"), Some("keep-alive"), "1.0 не знает, что соединение останется, пока ему не скажут");
        let r = ask(&mut client, &mut buf, "GET /2 HTTP/1.0\r\nConnection: keep-alive\r\n\r\n").await;
        assert_eq!(r.text(), "/2", "уговорённое соединение должно жить");
    }

    #[test]
    fn connection_header_is_a_list_of_tokens() {
        let with = |value: &[&str]| Request {
            method: "GET".to_string(),
            path: "/".to_string(),
            minor: 1,
            headers: value.iter().map(|v| ("Connection".to_string(), v.to_string())).collect(),
            body: Vec::new(),
        };
        assert!(wants_keep_alive(&with(&[])), "у 1.1 по умолчанию keep-alive");
        assert!(wants_keep_alive(&with(&["keep-alive, Upgrade"])));
        assert!(!wants_keep_alive(&with(&["Upgrade, CLOSE"])), "close среди токенов и в любом регистре");
        assert!(!wants_keep_alive(&with(&["keep-alive", "close"])), "close в любом из заголовков перевешивает");
        let mut old = with(&["Keep-Alive"]);
        old.minor = 0;
        assert!(wants_keep_alive(&old));
        old.headers.clear();
        assert!(!wants_keep_alive(&old));
    }
}
