use std::net::SocketAddr;
use std::time::{Duration, Instant};

use http_engine::{
    BodyReader, CorsPolicy, FileStore, Framing, Handler, Headers, Method, Params, RangeSpec, Request, Response, SameSite, ServerConfig, SetCookie, Version, apply,
    cookie, cors, etag, files_router, handler, parse_cookies, parse_range, serve_bytes, serve_with,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

#[derive(Debug)]
struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|l| {
            let (n, v) = l.split_once(':')?;
            n.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

struct Client {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Client {
    async fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).await.unwrap();
        stream.set_nodelay(true).unwrap();
        Client { stream, buf: Vec::new() }
    }

    async fn send(&mut self, raw: &str) {
        self.stream.write_all(raw.as_bytes()).await.unwrap();
        self.stream.flush().await.unwrap();
    }

    async fn fill(&mut self) -> bool {
        let mut chunk = [0u8; 4096];
        match timeout(Duration::from_secs(5), self.stream.read(&mut chunk)).await {
            Err(_) => panic!("сервер молчит больше 5 секунд"),
            Ok(Ok(0)) | Ok(Err(_)) => false,
            Ok(Ok(n)) => {
                self.buf.extend_from_slice(&chunk[..n]);
                true
            }
        }
    }

    async fn take(&mut self, n: usize) -> Vec<u8> {
        while self.buf.len() < n {
            assert!(self.fill().await, "соединение закрылось, не дослав тело");
        }
        self.buf.drain(..n).collect()
    }

    async fn line(&mut self) -> String {
        loop {
            if let Some(at) = self.buf.windows(2).position(|w| w == b"\r\n") {
                let line: Vec<u8> = self.buf.drain(..at + 2).collect();
                return String::from_utf8(line[..at].to_vec()).unwrap();
            }
            assert!(self.fill().await, "соединение закрылось посреди ответа");
        }
    }

    async fn reply_for(&mut self, head_request: bool) -> Option<Reply> {
        while !self.buf.windows(4).any(|w| w == b"\r\n\r\n") {
            if !self.fill().await {
                assert!(self.buf.is_empty(), "соединение закрыто посреди заголовков ответа: {:?}", String::from_utf8_lossy(&self.buf));
                return None;
            }
        }
        let at = self.buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let head = String::from_utf8(self.buf.drain(..at).collect()).unwrap();
        let status: u16 = head.split(' ').nth(1).and_then(|s| s.parse().ok()).expect("в ответе нет статуса");
        let mut reply = Reply { status, head, body: Vec::new() };
        if head_request || status == 100 || status == 204 || status == 304 {
            return Some(reply);
        }
        if reply.header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
            loop {
                let size = usize::from_str_radix(self.line().await.trim(), 16).expect("размер куска в hex");
                if size == 0 {
                    assert_eq!(self.line().await, "", "после нулевого куска должна быть пустая строка");
                    break;
                }
                reply.body.extend(self.take(size).await);
                assert_eq!(self.line().await, "", "после данных куска должен быть CRLF");
            }
        } else if let Some(n) = reply.header("content-length") {
            reply.body = self.take(n.parse().unwrap()).await;
        } else {
            while self.fill().await {}
            reply.body = std::mem::take(&mut self.buf);
        }
        Some(reply)
    }

    async fn reply(&mut self) -> Reply {
        self.reply_for(false).await.expect("сервер закрыл соединение, не ответив")
    }

    async fn call(&mut self, raw: &str) -> Reply {
        self.send(raw).await;
        self.reply().await
    }

    async fn assert_closed(&mut self, within: Duration) {
        let started = Instant::now();
        loop {
            let mut chunk = [0u8; 1024];
            match timeout(within, self.stream.read(&mut chunk)).await {
                Err(_) => panic!("сервер не закрыл соединение за {within:?}"),
                Ok(Ok(0)) | Ok(Err(_)) => return,
                Ok(Ok(n)) => {
                    assert!(self.buf.is_empty() && started.elapsed() < within, "после ответа сервер прислал лишние {n} байт");
                    self.buf.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }
}

fn request(method: Method, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Request {
    let mut h = Headers::new();
    for (n, v) in headers {
        h.append(n, v);
    }
    Request { method, target: target.to_string(), version: Version::Http11, headers: h, body: body.to_vec(), params: Params::new() }
}

fn header(r: &Response, name: &str) -> Option<String> {
    r.headers.get(name).map(str::to_string)
}

fn vary(r: &Response) -> Vec<String> {
    r.headers.get_all("vary").iter().flat_map(|v| v.split(',')).map(|t| t.trim().to_ascii_lowercase()).collect()
}

fn policy(origins: &[&str], credentials: bool) -> CorsPolicy {
    CorsPolicy {
        origins: origins.iter().map(|s| s.to_string()).collect(),
        methods: vec!["GET".into(), "PUT".into(), "DELETE".into()],
        headers: vec!["Content-Type".into(), "Authorization".into()],
        expose_headers: vec!["ETag".into()],
        credentials,
        max_age: 600,
    }
}

const APP: &str = "https://app.example";

fn inner(status: u16) -> Handler {
    handler(move |_| async move {
        let mut r = Response::new(status);
        r.headers.append("Vary", "Accept-Encoding");
        r
    })
}

#[test]
fn cookies_are_parsed_and_written() {
    let pairs = parse_cookies("theme=dark; token=eyJ=; bad name=1; sid=\"abc\"; sid=second");
    assert_eq!(cookie(&pairs, "theme"), Some("dark"));
    assert_eq!(cookie(&pairs, "token"), Some("eyJ="));
    assert_eq!(cookie(&pairs, "sid"), Some("abc"), "первое значение, кавычки сняты");
    assert_eq!(cookie(&pairs, "bad name"), None);
    let mut c = SetCookie::new("refresh", "t0k3n");
    c.max_age = Some(2_592_000);
    c.path = Some("/auth".into());
    c.http_only = true;
    c.secure = true;
    c.same_site = Some(SameSite::Lax);
    assert_eq!(c.to_header().unwrap(), "refresh=t0k3n; Max-Age=2592000; Path=/auth; HttpOnly; Secure; SameSite=Lax");
    assert_eq!(SetCookie::delete("refresh", "/auth").to_header().unwrap(), "refresh=; Max-Age=0; Path=/auth");
    assert!(SetCookie::new("a", "x y").to_header().is_err());
    let mut none = SetCookie::new("a", "1");
    none.same_site = Some(SameSite::None);
    assert!(none.to_header().is_err(), "SameSite=None без Secure");
}

#[tokio::test]
async fn cors_layer_answers_preflight_and_decorates_everything_else() {
    let app = apply(inner(500), &[cors(policy(&[APP], true))]);
    let pre = request(Method::Options, "/x", &[("Origin", APP), ("Access-Control-Request-Method", "PUT"), ("Access-Control-Request-Headers", "content-type, authorization")], b"");
    let r = app(pre).await;
    assert_eq!(r.status.code(), 204, "предзапрос обрабатывает слой, обработчик с его 500 не вызывается");
    assert_eq!(header(&r, "access-control-allow-origin").as_deref(), Some(APP));
    assert_eq!(header(&r, "access-control-allow-credentials").as_deref(), Some("true"));
    assert_eq!(header(&r, "access-control-max-age").as_deref(), Some("600"));
    for needed in ["origin", "access-control-request-method", "access-control-request-headers"] {
        assert!(vary(&r).contains(&needed.to_string()), "Vary без {needed}");
    }
    let bad = request(Method::Options, "/x", &[("Origin", APP), ("Access-Control-Request-Method", "PATCH")], b"");
    assert_eq!(app(bad).await.status.code(), 403);
    let evil = request(Method::Options, "/x", &[("Origin", "https://evil.example"), ("Access-Control-Request-Method", "GET")], b"");
    let r = app(evil).await;
    assert_eq!(r.status.code(), 403);
    assert!(header(&r, "access-control-allow-origin").is_none());

    let r = app(request(Method::Get, "/x", &[("Origin", APP)], b"")).await;
    assert_eq!(r.status.code(), 500);
    assert_eq!(header(&r, "access-control-allow-origin").as_deref(), Some(APP), "ответ об ошибке тоже должен быть читаем фронтом");
    assert_eq!(header(&r, "access-control-expose-headers").as_deref(), Some("ETag"));
    assert!(vary(&r).contains(&"origin".to_string()) && vary(&r).contains(&"accept-encoding".to_string()), "{:?}", vary(&r));
    let r = app(request(Method::Get, "/x", &[("Origin", "https://evil.example")], b"")).await;
    assert!(header(&r, "access-control-allow-origin").is_none());
    assert!(vary(&r).contains(&"origin".to_string()), "Vary нужен и чужому");
    let r = app(request(Method::Options, "/x", &[], b"")).await;
    assert_eq!(r.status.code(), 500, "обычный OPTIONS проходит к обработчику");
}

#[tokio::test]
async fn cors_wildcard_rules() {
    let open = apply(inner(200), &[cors(policy(&["*"], false))]);
    let r = open(request(Method::Get, "/x", &[("Origin", "https://any.example")], b"")).await;
    assert_eq!(header(&r, "access-control-allow-origin").as_deref(), Some("*"));
    assert!(!vary(&r).contains(&"origin".to_string()));
    let creds = apply(inner(200), &[cors(policy(&["*"], true))]);
    let r = creds(request(Method::Get, "/x", &[("Origin", "https://any.example")], b"")).await;
    assert_eq!(header(&r, "access-control-allow-origin").as_deref(), Some("https://any.example"), "'*' с credentials недопустима");
    assert_eq!(header(&r, "access-control-allow-credentials").as_deref(), Some("true"));
    assert!(vary(&r).contains(&"origin".to_string()));
}

#[tokio::test]
async fn bytes_are_served_conditionally_and_partially() {
    let data: Vec<u8> = (0..=255u8).collect();
    let tag = etag(&data);
    let r = serve_bytes(&request(Method::Get, "/f", &[], b""), "application/octet-stream", &data);
    assert_eq!((r.status.code(), r.body.len()), (200, 256));
    assert_eq!(header(&r, "etag").as_deref(), Some(tag.as_str()));
    assert_eq!(header(&r, "accept-ranges").as_deref(), Some("bytes"));
    let r = serve_bytes(&request(Method::Get, "/f", &[("If-None-Match", &format!("\"zzz\", W/{tag}"))], b""), "application/octet-stream", &data);
    assert_eq!(r.status.code(), 304);
    assert!(r.body.is_empty());
    assert_eq!(header(&r, "etag").as_deref(), Some(tag.as_str()));
    let r = serve_bytes(&request(Method::Get, "/f", &[("Range", "bytes=250-")], b""), "application/octet-stream", &data);
    assert_eq!((r.status.code(), r.body.clone()), (206, data[250..].to_vec()));
    assert_eq!(header(&r, "content-range").as_deref(), Some("bytes 250-255/256"));
    let r = serve_bytes(&request(Method::Get, "/f", &[("Range", "bytes=-4")], b""), "application/octet-stream", &data);
    assert_eq!(r.body, &data[252..]);
    let r = serve_bytes(&request(Method::Get, "/f", &[("Range", "bytes=999-")], b""), "application/octet-stream", &data);
    assert_eq!(r.status.code(), 416);
    assert_eq!(header(&r, "content-range").as_deref(), Some("bytes */256"));
    let r = serve_bytes(&request(Method::Get, "/f", &[("Range", "bytes=5-2")], b""), "application/octet-stream", &data);
    assert_eq!(r.status.code(), 200, "испорченный диапазон игнорируется");
    let r = serve_bytes(&request(Method::Get, "/f", &[("If-None-Match", &tag), ("Range", "bytes=0-9")], b""), "application/octet-stream", &data);
    assert_eq!(r.status.code(), 304, "условие проверяется раньше диапазона");
    assert_eq!(parse_range(Some("bytes=0-0"), 1), RangeSpec::Partial { start: 0, end: 0 });
    assert_eq!(parse_range(Some("bytes=0-"), 0), RangeSpec::Unsatisfiable);
}

#[tokio::test]
async fn body_reader_streams_and_stops_floods() {
    let wire: &[u8] = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\nNEXT";
    let mut body = BodyReader::new(wire, Vec::new(), Framing::Chunked, 100).unwrap();
    let mut all = Vec::new();
    while let Some(piece) = body.next_chunk().await.unwrap() {
        all.extend_from_slice(&piece);
    }
    assert_eq!(all, b"hello world");
    assert!(body.is_done());
    let (mut rest, buf) = body.into_parts();
    let mut tail = Vec::new();
    rest.read_to_end(&mut tail).await.unwrap();
    assert_eq!([buf, tail].concat(), b"NEXT");

    assert!(BodyReader::new(&b""[..], Vec::new(), Framing::Length(5000), 1000).is_err(), "заявлено больше лимита: отказ без чтения");
    let big = vec![b'x'; 20_000];
    let mut body = BodyReader::new(&big[..], Vec::new(), Framing::Length(20_000), 1 << 20).unwrap();
    let mut biggest = 0;
    let mut total = 0;
    while let Some(piece) = body.next_chunk().await.unwrap() {
        biggest = biggest.max(piece.len());
        total += piece.len();
    }
    assert_eq!(total, 20_000);
    assert!(biggest <= 4096, "кусок {biggest}");
    let mut flood = BodyReader::new(&b"ffff\r\n"[..], Vec::new(), Framing::Chunked, 1000).unwrap();
    assert!(flood.next_chunk().await.is_err(), "размер куска больше лимита");
    let truncated = BodyReader::new(&b"abc"[..], Vec::new(), Framing::Length(10), 100);
    let mut truncated = truncated.unwrap();
    let mut failed = false;
    for _ in 0..4 {
        if truncated.next_chunk().await.is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed, "обрыв посреди тела это ошибка");
    let mut skipped = BodyReader::new(&b"0123456789"[..], Vec::new(), Framing::Length(10), 100).unwrap();
    assert!(skipped.drain(100).await.unwrap(), "небольшое тело вычитывается");
    let mut too_long = BodyReader::new(&big[..], Vec::new(), Framing::Length(20_000), 1 << 20).unwrap();
    assert!(!too_long.drain(5000).await.unwrap(), "больше предела вычитывать не будем");
}

async fn start(max_upload: usize) -> (SocketAddr, FileStore, tokio::sync::oneshot::Sender<()>) {
    let store = FileStore::new();
    let app = apply(files_router(store.clone(), max_upload).into_handler(), &[cors(policy(&[APP], false))]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = serve_with(listener, app, ServerConfig::default(), async move {
            let _ = stopped.await;
        })
        .await;
    });
    (addr, store, stop)
}

fn put(path: &str, content_type: &str, body: &[u8]) -> String {
    format!("PUT {path} HTTP/1.1\r\nHost: t\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n{}", body.len(), String::from_utf8_lossy(body))
}

#[tokio::test]
async fn file_service_end_to_end() {
    let (addr, store, _stop) = start(1000).await;
    let mut c = Client::connect(addr).await;
    let created = c.call(&put("/files/notes.txt", "text/plain", b"0123456789abcdef")).await;
    assert_eq!(created.status, 201, "{created:?}");
    assert_eq!(c.call(&put("/files/notes.txt", "text/plain", b"0123456789abcdef")).await.status, 204, "повторная загрузка заменяет файл");
    assert_eq!(store.len(), 1);

    let full = c.call("GET /files/notes.txt HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert_eq!((full.status, full.text()), (200, "0123456789abcdef".to_string()));
    assert_eq!(full.header("content-type").as_deref(), Some("text/plain"));
    assert_eq!(full.header("accept-ranges").as_deref(), Some("bytes"));
    let tag = full.header("etag").expect("у файла есть ETag");

    let fresh = c.call(&format!("GET /files/notes.txt HTTP/1.1\r\nHost: t\r\nIf-None-Match: {tag}\r\n\r\n")).await;
    assert_eq!(fresh.status, 304);
    assert!(fresh.body.is_empty());

    let part = c.call("GET /files/notes.txt HTTP/1.1\r\nHost: t\r\nRange: bytes=10-\r\n\r\n").await;
    assert_eq!((part.status, part.text()), (206, "abcdef".to_string()));
    assert_eq!(part.header("content-range").as_deref(), Some("bytes 10-15/16"));
    let bad = c.call("GET /files/notes.txt HTTP/1.1\r\nHost: t\r\nRange: bytes=100-\r\n\r\n").await;
    assert_eq!(bad.status, 416);

    c.send("HEAD /files/notes.txt HTTP/1.1\r\nHost: t\r\n\r\n").await;
    let head = c.reply_for(true).await.unwrap();
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-length").as_deref(), Some("16"));
    assert_eq!(c.call("GET /files/missing HTTP/1.1\r\nHost: t\r\n\r\n").await.status, 404);

    let big = vec![b'x'; 2000];
    let refused = c.call(&put("/files/big.bin", "application/octet-stream", &big)).await;
    assert_eq!(refused.status, 413);
    assert_eq!(c.call("GET /files/notes.txt HTTP/1.1\r\nHost: t\r\nRange: bytes=0-1\r\n\r\n").await.text(), "01", "после отказа соединение живо и не путается");

    assert_eq!(c.call("DELETE /files/notes.txt HTTP/1.1\r\nHost: t\r\n\r\n").await.status, 204);
    assert_eq!(c.call("DELETE /files/notes.txt HTTP/1.1\r\nHost: t\r\n\r\n").await.status, 404);
    assert!(store.is_empty());
}

#[tokio::test]
async fn browser_flow_over_the_wire() {
    let (addr, _store, _stop) = start(1000).await;
    let mut c = Client::connect(addr).await;
    let pre = c
        .call("OPTIONS /files/a.txt HTTP/1.1\r\nHost: t\r\nOrigin: https://app.example\r\nAccess-Control-Request-Method: PUT\r\nAccess-Control-Request-Headers: content-type\r\n\r\n")
        .await;
    assert_eq!(pre.status, 204, "{pre:?}");
    assert_eq!(pre.header("access-control-allow-origin").as_deref(), Some(APP));
    let up = c.call("PUT /files/a.txt HTTP/1.1\r\nHost: t\r\nOrigin: https://app.example\r\nContent-Length: 2\r\n\r\nhi").await;
    assert_eq!(up.status, 201);
    assert_eq!(up.header("access-control-allow-origin").as_deref(), Some(APP));
    let miss = c.call("GET /files/zzz HTTP/1.1\r\nHost: t\r\nOrigin: https://app.example\r\n\r\n").await;
    assert_eq!(miss.status, 404);
    assert_eq!(miss.header("access-control-allow-origin").as_deref(), Some(APP), "404 тоже читаем фронтом");
    let other = c.call("GET /files/a.txt HTTP/1.1\r\nHost: t\r\nOrigin: https://evil.example\r\n\r\n").await;
    assert_eq!(other.status, 200);
    assert!(other.header("access-control-allow-origin").is_none(), "чужой сайт ответ прочитать не сможет");
}

#[tokio::test]
async fn graceful_server_still_works_with_new_layers() {
    let (addr, _store, stop) = start(1000).await;
    let started = Instant::now();
    let mut c = Client::connect(addr).await;
    assert_eq!(c.call("GET /files/none HTTP/1.1\r\nHost: t\r\n\r\n").await.status, 404);
    drop(stop);
    c.assert_closed(Duration::from_secs(5)).await;
    assert!(started.elapsed() < Duration::from_secs(5));
}
