use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, PartialEq)]
struct Request {
    method: String,
    path: String,
    query: Option<String>,
}

struct Response {
    status: u16,
    body: String,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "",
    }
}

fn parse_request_line(line: &str) -> Option<Request> {
    let mut parts = line.split(' ');
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !version.starts_with("HTTP/1.") || !target.starts_with('/') || method.is_empty() {
        return None;
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query.to_string())),
        None => (target, None),
    };
    Some(Request { method: method.to_string(), path: path.to_string(), query })
}

async fn read_head(stream: &mut TcpStream) -> io::Result<Option<String>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

impl Response {
    fn to_bytes(&self) -> Vec<u8> {
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            reason(self.status),
            self.body.len(),
            self.body
        )
        .into_bytes()
    }
}

fn site(req: &Request) -> Response {
    let text = |status, body: &str| Response { status, body: body.to_string() };
    match req.path.as_str() {
        "/" => text(200, "Привет, мир"),
        "/method" => text(200, &req.method),
        path => match path.strip_prefix("/echo/") {
            Some(rest) => text(200, rest),
            None => text(404, &format!("no such page: {path}")),
        },
    }
}

async fn serve_connection(mut stream: TcpStream, handler: fn(&Request) -> Response) -> io::Result<()> {
    let Some(head) = read_head(&mut stream).await? else {
        return Ok(());
    };
    let line = head.lines().next().unwrap_or("");
    let response = match parse_request_line(line) {
        Some(request) => handler(&request),
        None => Response { status: 400, body: "bad request".to_string() },
    };
    stream.write_all(&response.to_bytes()).await?;
    stream.shutdown().await
}

async fn serve(listener: TcpListener, handler: fn(&Request) -> Response) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let _ = serve_connection(stream, handler).await;
        });
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    serve(listener, site).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::timeout;

    async fn start(handler: fn(&Request) -> Response) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, handler));
        addr
    }

    async fn ask(addr: SocketAddr, raw: &str) -> String {
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(raw.as_bytes()).await.unwrap();
        let mut all = Vec::new();
        timeout(Duration::from_secs(2), client.read_to_end(&mut all))
            .await
            .expect("сервер не закрыл соединение после ответа")
            .unwrap();
        String::from_utf8(all).unwrap()
    }

    fn req(method: &str, path: &str, query: Option<&str>) -> Request {
        Request { method: method.to_string(), path: path.to_string(), query: query.map(str::to_string) }
    }

    #[test]
    fn site_pages() {
        assert_eq!(site(&req("GET", "/", None)).body, "Привет, мир");
        assert_eq!(site(&req("GET", "/echo/привет", None)).body, "привет");
        assert_eq!(site(&req("GET", "/echo/", None)).status, 200, "пустой текст после /echo/ это пустое эхо, а не 404");
        assert_eq!(site(&req("POST", "/method", None)).body, "POST");
        let missing = site(&req("GET", "/nope", None));
        assert_eq!((missing.status, missing.body.as_str()), (404, "no such page: /nope"));
        assert_eq!(site(&req("GET", "/echo", None)).status, 404, "/echo без слеша это не эхо");
    }

    #[test]
    fn serialization_counts_bytes() {
        let text = String::from_utf8(Response { status: 200, body: "Привет, мир".to_string() }.to_bytes()).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "строка статуса: {text:?}");
        assert!(
            text.contains("Content-Length: 20\r\n"),
            "Content-Length считает байты, не символы: в UTF-8 это 20, а не 11: {text:?}"
        );
        assert!(text.ends_with("\r\n\r\nПривет, мир"));
        let empty = String::from_utf8(Response { status: 404, body: String::new() }.to_bytes()).unwrap();
        assert!(empty.starts_with("HTTP/1.1 404 Not Found\r\n") && empty.contains("Content-Length: 0\r\n"), "{empty:?}");
    }

    #[tokio::test]
    async fn serves_over_tcp() {
        let addr = start(site).await;
        let text = ask(addr, "GET /echo/hi?x=1 HTTP/1.1\r\nHost: a\r\n\r\n").await;
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text:?}");
        assert!(text.ends_with("\r\n\r\nhi"), "query не должен попадать в путь: {text:?}");
        let text = ask(addr, "DELETE /method HTTP/1.1\r\nHost: a\r\n\r\n").await;
        assert!(text.ends_with("DELETE"), "{text:?}");
    }

    static BAD_REQUEST_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn counting(_req: &Request) -> Response {
        BAD_REQUEST_CALLS.fetch_add(1, Ordering::SeqCst);
        Response { status: 200, body: "ok".to_string() }
    }

    #[tokio::test]
    async fn unreadable_request_gets_400_without_calling_the_handler() {
        let addr = start(counting).await;
        for raw in ["GARBAGE\r\n\r\n", "GET nopath HTTP/1.1\r\n\r\n", "GET / HTTP/1.1 extra\r\n\r\n"] {
            let text = ask(addr, raw).await;
            assert!(text.starts_with("HTTP/1.1 400 Bad Request\r\n"), "{raw:?} должен получить 400, получил {text:?}");
        }
        assert_eq!(BAD_REQUEST_CALLS.load(Ordering::SeqCst), 0, "обработчик не должен видеть запрос, который не разобрался");
        assert!(ask(addr, "GET / HTTP/1.1\r\nHost: a\r\n\r\n").await.ends_with("ok"));
        assert_eq!(BAD_REQUEST_CALLS.load(Ordering::SeqCst), 1);
    }
}
