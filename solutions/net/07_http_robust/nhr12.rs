use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;

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

type App = Arc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;

fn wants_keep_alive(request: &Request) -> bool {
    let tokens: Vec<String> = request
        .headers
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case("connection"))
        .flat_map(|(_, v)| v.split(',').map(|t| t.trim().to_ascii_lowercase()))
        .collect();
    !tokens.iter().any(|t| t == "close") && (request.minor == 1 || tokens.iter().any(|t| t == "keep-alive"))
}


async fn serve<S>(listener: TcpListener, app: App, shutdown: S)
where
    S: Future<Output = ()>,
{
    let (stop_tx, stop_rx) = watch::channel(false);
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    connections.spawn(connection(stream, app.clone(), stop_rx.clone()));
                }
            }
        }
    }
    drop(listener);
    let _ = stop_tx.send(true);
    while connections.join_next().await.is_some() {}
}

async fn connection(mut stream: TcpStream, app: App, mut stop: watch::Receiver<bool>) {
    let mut buf = Vec::new();
    loop {
        if buf.is_empty() {
            if *stop.borrow() {
                return;
            }
            tokio::select! {
                _ = stop.changed() => return,
                ready = stream.readable() => {
                    if ready.is_err() {
                        return;
                    }
                }
            }
        }
        let Ok(Some(request)) = read_request(&mut stream, &mut buf).await else {
            return;
        };
        let keep = wants_keep_alive(&request);
        let mut response = app(request).await;
        let keep = keep && !*stop.borrow();
        if !keep {
            response = response.header("Connection", "close");
        }
        if write_response(&mut stream, &response).await.is_err() || !keep {
            return;
        }
    }
}

fn demo_app() -> App {
    Arc::new(|request| {
        Box::pin(async move {
            if request.path == "/slow" {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Response::new(200, &request.path)
        })
    })
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    serve(listener, demo_app(), async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;
    use tokio::time::timeout;

    struct Server {
        addr: SocketAddr,
        stop: Option<oneshot::Sender<()>>,
        done: Arc<AtomicBool>,
        task: JoinHandle<()>,
    }

    async fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let done = Arc::new(AtomicBool::new(false));
        let marker = done.clone();
        let task = tokio::spawn(async move {
            serve(listener, demo_app(), async {
                let _ = rx.await;
            })
            .await;
            marker.store(true, Ordering::SeqCst);
        });
        Server { addr, stop: Some(tx), done, task }
    }

    impl Server {
        fn stop(&mut self) {
            self.stop.take().expect("сигнал уже послан").send(()).unwrap();
        }
    }

    struct Client {
        stream: TcpStream,
        buf: Vec<u8>,
    }

    impl Client {
        async fn connect(addr: SocketAddr) -> Client {
            Client { stream: TcpStream::connect(addr).await.unwrap(), buf: Vec::new() }
        }

        async fn get(&mut self, path: &str) {
            self.stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n").as_bytes()).await.unwrap();
        }

        async fn reply(&mut self) -> Option<(String, String)> {
            loop {
                if let Some(end) = find_head_end(&self.buf) {
                    let head = String::from_utf8(self.buf[..end].to_vec()).unwrap();
                    let len: usize = head
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap()))
                        .unwrap_or(0);
                    if self.buf.len() >= end + len {
                        let body = String::from_utf8(self.buf[end..end + len].to_vec()).unwrap();
                        self.buf.drain(..end + len);
                        return Some((head, body));
                    }
                }
                let mut chunk = [0u8; 1024];
                let n = timeout(Duration::from_secs(3), self.stream.read(&mut chunk)).await.expect("сервер молчит больше 3 секунд").unwrap_or(0);
                if n == 0 {
                    assert!(self.buf.is_empty(), "соединение оборвано посреди ответа: {:?}", String::from_utf8_lossy(&self.buf));
                    return None;
                }
                self.buf.extend_from_slice(&chunk[..n]);
            }
        }
    }

    #[tokio::test]
    async fn idle_keep_alive_connections_close_at_once() {
        let mut server = start().await;
        let mut idle = Client::connect(server.addr).await;
        idle.get("/hello").await;
        assert_eq!(idle.reply().await.unwrap().1, "/hello");
        server.stop();
        let gone = timeout(Duration::from_millis(500), idle.reply()).await.expect("простаивающее соединение держит остановку: закрывать его надо сразу");
        assert!(gone.is_none(), "лишних байт после остановки быть не должно");
        timeout(Duration::from_secs(2), server.task).await.expect("serve не вернулся после закрытия всех соединений").unwrap();
    }

    #[tokio::test]
    async fn request_in_flight_is_finished_politely() {
        let mut server = start().await;
        let mut busy = Client::connect(server.addr).await;
        busy.get("/slow").await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        server.stop();
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!server.done.load(Ordering::SeqCst), "serve завершился, не дождавшись запроса в работе");
        let (head, body) = busy.reply().await.expect("запрос в работе потерян");
        assert_eq!(body, "/slow", "ответ должен прийти полностью");
        assert!(head.to_ascii_lowercase().contains("connection: close"), "останавливаясь, сервер говорит, что соединение закроется: {head:?}");
        assert!(timeout(Duration::from_secs(1), busy.reply()).await.expect("соединение не закрылось").is_none());
        timeout(Duration::from_secs(2), server.task).await.expect("serve не завершился").unwrap();
        assert!(server.done.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn new_clients_are_not_served_after_the_signal() {
        let mut server = start().await;
        server.stop();
        timeout(Duration::from_secs(2), &mut server.task).await.expect("пустой сервер должен остановиться сразу").unwrap();
        if let Ok(mut late) = TcpStream::connect(server.addr).await {
            let _ = late.write_all(b"GET /late HTTP/1.1\r\nHost: t\r\n\r\n").await;
            let mut out = Vec::new();
            let read = timeout(Duration::from_secs(1), late.read_to_end(&mut out)).await;
            assert!(read.is_err() || out.is_empty(), "после остановки новый клиент получил ответ: {:?}", String::from_utf8_lossy(&out));
        }
    }

    #[tokio::test]
    async fn busy_and_idle_clients_together() {
        let mut server = start().await;
        let mut clients = Vec::new();
        for i in 0..6 {
            let mut c = Client::connect(server.addr).await;
            c.get(if i % 2 == 0 { "/slow" } else { "/fast" }).await;
            clients.push(c);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
        server.stop();
        for (i, c) in clients.iter_mut().enumerate() {
            let (_, body) = c.reply().await.expect("ответ потерян");
            assert_eq!(body, if i % 2 == 0 { "/slow" } else { "/fast" });
            assert!(timeout(Duration::from_secs(1), c.reply()).await.expect("соединение не закрылось").is_none(), "клиент {i}");
        }
        timeout(Duration::from_secs(2), server.task).await.unwrap().unwrap();
    }
}
