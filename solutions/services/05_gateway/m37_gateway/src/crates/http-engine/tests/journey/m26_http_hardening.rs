use std::collections::HashMap;
use std::net::SocketAddr;
use std::panic::resume_unwind;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use http_engine::{
    Handler, Limits, Method, Request, Response, Router, Server, ServerConfig, access_log, apply,
    bearer_auth, handler,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
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
        let stream = TcpStream::connect(addr)
            .await
            .expect("сервер принимает соединения");
        stream.set_nodelay(true).expect("nodelay ставится");
        Client {
            stream,
            buf: Vec::new(),
        }
    }

    async fn send(&mut self, raw: &str) {
        self.stream
            .write_all(raw.as_bytes())
            .await
            .expect("запрос уходит");
        self.stream.flush().await.expect("запрос уходит");
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
                return String::from_utf8(line[..at].to_vec()).expect("строка ответа в UTF-8");
            }
            assert!(self.fill().await, "соединение закрылось посреди ответа");
        }
    }

    async fn reply_for(&mut self, head_request: bool) -> Option<Reply> {
        while !self.buf.windows(4).any(|w| w == b"\r\n\r\n") {
            if !self.fill().await {
                assert!(
                    self.buf.is_empty(),
                    "соединение закрыто посреди заголовков ответа: {:?}",
                    String::from_utf8_lossy(&self.buf)
                );
                return None;
            }
        }
        let at = self
            .buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .unwrap_or_default()
            + 4;
        let head =
            String::from_utf8(self.buf.drain(..at).collect()).expect("заголовки ответа в UTF-8");
        let status: u16 = head
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("в ответе нет статуса");
        let mut reply = Reply {
            status,
            head,
            body: Vec::new(),
        };
        if head_request || status == 100 || status == 204 || status == 304 {
            return Some(reply);
        }
        if reply
            .header("transfer-encoding")
            .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
        {
            loop {
                let size = usize::from_str_radix(self.line().await.trim(), 16)
                    .expect("размер куска в hex");
                if size == 0 {
                    assert_eq!(
                        self.line().await,
                        "",
                        "после нулевого куска должна быть пустая строка"
                    );
                    break;
                }
                reply.body.extend(self.take(size).await);
                assert_eq!(self.line().await, "", "после данных куска должен быть CRLF");
            }
        } else if let Some(n) = reply.header("content-length") {
            reply.body = self.take(n.parse().expect("Content-Length числом")).await;
        } else {
            while self.fill().await {}
            reply.body = std::mem::take(&mut self.buf);
        }
        Some(reply)
    }

    async fn reply(&mut self) -> Reply {
        self.reply_for(false)
            .await
            .expect("сервер закрыл соединение, не ответив")
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
                    assert!(
                        self.buf.is_empty() && started.elapsed() < within,
                        "после ответа сервер прислал лишние {n} байт"
                    );
                    self.buf.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n")
}

fn base_router() -> Router {
    Router::new()
        .route(
            Method::Get,
            "/hello",
            handler(|_| async { Response::text(200, "hello") }),
        )
        .route(
            Method::Get,
            "/slow",
            handler(|_| async {
                tokio::time::sleep(Duration::from_millis(250)).await;
                Response::text(200, "slow")
            }),
        )
        .route(Method::Get, "/boom", handler(|_| async { explode() }))
        .route(
            Method::Post,
            "/echo",
            handler(
                |r: Request| async move { Response::text(200, &String::from_utf8_lossy(&r.body)) },
            ),
        )
        .route(
            Method::Get,
            "/me",
            handler(|r: Request| async move {
                Response::text(200, r.headers.get("x-user").unwrap_or("nobody"))
            }),
        )
}

fn explode() -> Response {
    resume_unwind(Box::new("boom"))
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn quiet() -> ServerConfig {
    ServerConfig {
        limits: Limits {
            max_head_bytes: 4096,
            max_headers: 32,
            max_body: 1000,
        },
        idle_timeout: Duration::from_secs(30),
        head_timeout: Duration::from_secs(30),
        body_timeout: Duration::from_secs(30),
        max_connections: 64,
        drain_timeout: Duration::from_secs(5),
        proxy_protocol: false,
    }
}

async fn start(app: Handler, config: ServerConfig) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен всегда");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    let server = Server::with_handler(app).config(config);
    tokio::spawn(server.serve(listener, std::future::pending()));
    addr
}

#[tokio::test]
async fn keep_alive_serves_many_requests_on_one_connection() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    for i in 0..3 {
        let reply = c.call(&get("/hello")).await;
        assert_eq!(
            (reply.status, reply.text()),
            (200, "hello".to_string()),
            "запрос номер {i} на том же соединении"
        );
        assert_ne!(
            reply.header("connection").as_deref(),
            Some("close"),
            "keep-alive не должен объявлять закрытие"
        );
    }
    c.send("HEAD /hello HTTP/1.1\r\nHost: t\r\n\r\n").await;
    let head = c.reply_for(true).await.expect("ответ на HEAD");
    assert_eq!(head.status, 200);
    assert_eq!(
        c.call(&get("/hello")).await.text(),
        "hello",
        "после ответа на HEAD не должно остаться лишних байт, иначе следующий ответ разберётся неверно"
    );
    let last = c
        .call("GET /hello HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
        .await;
    assert_eq!(
        last.header("connection").as_deref(),
        Some("close"),
        "закрывая, сервер должен об этом сказать"
    );
    c.assert_closed(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn http10_closes_unless_asked_otherwise() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    let reply = c.call("GET /hello HTTP/1.0\r\n\r\n").await;
    assert_eq!(reply.text(), "hello");
    c.assert_closed(Duration::from_secs(2)).await;

    let mut c = Client::connect(addr).await;
    let reply = c
        .call("GET /hello HTTP/1.0\r\nConnection: keep-alive\r\n\r\n")
        .await;
    assert_eq!(
        reply.header("connection").as_deref(),
        Some("keep-alive"),
        "HTTP/1.0 с keep-alive должен получить подтверждение"
    );
    assert_eq!(
        c.call("GET /hello HTTP/1.0\r\nConnection: keep-alive\r\n\r\n")
            .await
            .text(),
        "hello"
    );
}

#[tokio::test]
async fn pipelined_requests_get_ordered_answers() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    let body = "ping";
    let all = format!(
        "{}{}POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n{body}{}",
        get("/slow"),
        get("/hello"),
        body.len(),
        &get("/hello")[..10]
    );
    c.send(&all).await;
    assert_eq!(
        c.reply().await.text(),
        "slow",
        "ответы идут строго в порядке запросов, даже если второй готов раньше"
    );
    assert_eq!(c.reply().await.text(), "hello");
    assert_eq!(
        c.reply().await.text(),
        "ping",
        "тело третьего запроса не должно потеряться в остатке буфера"
    );
    c.send(&get("/hello")[10..]).await;
    assert_eq!(
        c.reply().await.text(),
        "hello",
        "половина четвёртого запроса осталась в буфере и достроилась"
    );
}

#[tokio::test]
async fn idle_connections_close_quietly() {
    let config = ServerConfig {
        idle_timeout: Duration::from_millis(300),
        ..quiet()
    };
    let addr = start(base_router().into_handler(), config).await;
    let mut c = Client::connect(addr).await;
    assert_eq!(c.call(&get("/hello")).await.status, 200);
    c.assert_closed(Duration::from_secs(2)).await;

    let mut silent = Client::connect(addr).await;
    silent.assert_closed(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn slowloris_gets_408_by_total_deadline() {
    let config = ServerConfig {
        head_timeout: Duration::from_millis(500),
        idle_timeout: Duration::from_secs(30),
        ..quiet()
    };
    let addr = start(base_router().into_handler(), config).await;
    let mut c = Client::connect(addr).await;
    c.send("GET /hello HTTP/1.1\r\nHost: t\r\n").await;
    let started = Instant::now();
    let mut dripped = 0;
    let reply = loop {
        if started.elapsed() > Duration::from_secs(3) {
            panic!(
                "за {dripped} капель сервер так и не оборвал медленного клиента: нужен общий дедлайн на голову, а не таймаут на каждый read"
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        if c.stream.write_all(b"X-Drip: a\r\n").await.is_err() {
            break c.reply_for(false).await;
        }
        dripped += 1;
        if let Ok(Some(reply)) = timeout(Duration::from_millis(1), c.reply_for(false)).await {
            break Some(reply);
        }
    };
    let reply = reply.expect("перед закрытием ждали ответ 408");
    assert_eq!(reply.status, 408);
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "408 пришёл слишком поздно: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn stalled_body_gets_408() {
    let config = ServerConfig {
        body_timeout: Duration::from_millis(300),
        ..quiet()
    };
    let addr = start(base_router().into_handler(), config).await;
    let mut c = Client::connect(addr).await;
    c.send("POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 10\r\n\r\nabc")
        .await;
    let reply = c.reply().await;
    assert_eq!(reply.status, 408, "тело обещали, но не дослали");
    c.assert_closed(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn limits_reject_early() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    let mut big = String::from("GET /hello HTTP/1.1\r\nHost: t\r\n");
    while big.len() < 20_000 {
        big.push_str("X-Filler: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\r\n");
    }
    let _ = c.stream.write_all(big.as_bytes()).await;
    assert_eq!(c.reply().await.status, 431);
    c.assert_closed(Duration::from_secs(3)).await;

    let mut c = Client::connect(addr).await;
    let reply = c
        .call("POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 5000\r\n\r\n")
        .await;
    assert_eq!(
        reply.status, 413,
        "заявленная длина больше лимита: отвечаем до получения тела"
    );
    c.assert_closed(Duration::from_secs(2)).await;
}

#[tokio::test]
async fn chunked_request_bodies() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    c.send("POST /echo HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel")
        .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    c.send("lo\r\n6;ext=1\r\n world\r\n0\r\nX-T: 1\r\n\r\n")
        .await;
    let reply = c.reply().await;
    assert_eq!(
        reply.text(),
        "hello world",
        "куски склеиваются в одно тело, расширения и трейлеры не мешают"
    );
    assert_eq!(
        c.call(&get("/hello")).await.text(),
        "hello",
        "после chunked-запроса соединение остаётся рабочим"
    );

    let mut c = Client::connect(addr).await;
    let reply = c
        .call("POST /echo HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n3E9\r\n")
        .await;
    assert_eq!(
        reply.status, 413,
        "заявленный кусок больше лимита: отвечаем, не дожидаясь его данных"
    );
}

#[tokio::test]
async fn expect_continue_flow() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    c.send("POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\nExpect: 100-continue\r\n\r\n")
        .await;
    let interim = timeout(Duration::from_millis(800), c.reply_for(false))
        .await
        .expect("клиент ждёт 100 Continue перед отправкой тела, а сервер молчит (curl прождал бы секунду)")
        .expect("промежуточный ответ 100");
    assert_eq!(interim.status, 100);
    c.send("hello").await;
    let reply = c.reply().await;
    assert_eq!((reply.status, reply.text()), (200, "hello".to_string()));

    let mut c = Client::connect(addr).await;
    c.send(
        "POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 5000\r\nExpect: 100-continue\r\n\r\n",
    )
    .await;
    let reply = c.reply().await;
    assert_eq!(
        reply.status, 413,
        "если тело всё равно не примем, 100 Continue слать нельзя: сразу отказ"
    );

    let mut c = Client::connect(addr).await;
    let reply = c
        .call("POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 1\r\nExpect: magic\r\n\r\nx")
        .await;
    assert_eq!(reply.status, 417, "незнакомое ожидание это 417");
}

#[tokio::test]
async fn streaming_responses_are_chunked_and_incremental() {
    let tx_slot = Arc::new(Mutex::new(None::<mpsc::Sender<Vec<u8>>>));
    let slot = tx_slot.clone();
    let stream_handler = handler(move |_| {
        let slot = slot.clone();
        async move {
            let (tx, rx) = mpsc::channel(8);
            *locked(&slot) = Some(tx);
            Response::streaming(200, "text/plain", rx)
        }
    });
    let router = Router::new()
        .route(Method::Get, "/stream", stream_handler)
        .route(
            Method::Get,
            "/hello",
            handler(|_| async { Response::text(200, "hello") }),
        );
    let addr = start(router.into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    c.send(&get("/stream")).await;
    let mut head = String::new();
    loop {
        let line = c.line().await;
        if line.is_empty() {
            break;
        }
        head.push_str(&line.to_ascii_lowercase());
        head.push('\n');
    }
    assert!(
        head.contains("transfer-encoding: chunked"),
        "тело неизвестной длины идёт chunked: {head}"
    );
    assert!(
        !head.contains("content-length"),
        "у потока длины быть не может: {head}"
    );
    let tx = loop {
        if let Some(tx) = locked(&tx_slot).take() {
            break tx;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    tx.send(b"hello".to_vec())
        .await
        .expect("кусок уходит в поток");
    assert_eq!(
        c.line().await,
        "5",
        "первый кусок должен уйти клиенту, пока поток не закончился"
    );
    assert_eq!(c.line().await, "hello");
    tx.send(Vec::new()).await.expect("кусок уходит в поток");
    tx.send(b" world".to_vec())
        .await
        .expect("кусок уходит в поток");
    assert_eq!(
        c.line().await,
        "6",
        "пустой кусок пропускается: нулевой размер закрыл бы тело досрочно"
    );
    assert_eq!(c.line().await, " world");
    drop(tx);
    assert_eq!(c.line().await, "0");
    assert_eq!(c.line().await, "");
    assert_eq!(
        c.call(&get("/hello")).await.text(),
        "hello",
        "после потока соединение остаётся пригодным"
    );
}

#[tokio::test]
async fn streaming_to_http10_is_raw_and_closes() {
    let router = Router::new().route(
        Method::Get,
        "/stream",
        handler(|_| async {
            let (tx, rx) = mpsc::channel(8);
            tokio::spawn(async move {
                let _ = tx.send(b"ab".to_vec()).await;
                let _ = tx.send(b"cd".to_vec()).await;
            });
            Response::streaming(200, "text/plain", rx)
        }),
    );
    let addr = start(router.into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    let reply = c.call("GET /stream HTTP/1.0\r\n\r\n").await;
    assert!(
        reply.header("transfer-encoding").is_none(),
        "HTTP/1.0 не знает chunked"
    );
    assert_eq!(
        reply.text(),
        "abcd",
        "для 1.0 тело идёт как есть и кончается закрытием соединения"
    );
}

#[tokio::test]
async fn panic_keeps_the_connection_usable() {
    let addr = start(base_router().into_handler(), quiet()).await;
    let mut c = Client::connect(addr).await;
    assert_eq!(c.call(&get("/boom")).await.status, 500);
    assert_eq!(
        c.call(&get("/hello")).await.text(),
        "hello",
        "после 500 на keep-alive соединении можно продолжать"
    );
}

#[tokio::test]
async fn middleware_order_and_auth() {
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = log.clone();
    let tokens = HashMap::from([("t-ada".to_string(), "ada".to_string())]);
    let layers = vec![
        access_log(Arc::new(move |line| locked(&sink).push(line))),
        bearer_auth(
            move |token| tokens.get(token).cloned(),
            |request: &Request| request.raw_path() != "/hello",
        ),
    ];
    let app = apply(base_router().into_handler(), &layers);
    let addr = start(app, quiet()).await;
    let mut c = Client::connect(addr).await;

    let denied = c.call(&get("/me")).await;
    assert_eq!(denied.status, 401);
    assert_eq!(denied.header("www-authenticate").as_deref(), Some("Bearer"));
    let wrong = c
        .call("GET /me HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer nope\r\n\r\n")
        .await;
    assert_eq!(wrong.status, 401, "неизвестный токен");
    let basic = c
        .call("GET /me HTTP/1.1\r\nHost: t\r\nAuthorization: Basic dDphZGE=\r\n\r\n")
        .await;
    assert_eq!(basic.status, 401, "чужая схема авторизации");
    let ok = c
        .call("GET /me HTTP/1.1\r\nHost: t\r\nauthorization: bearer t-ada\r\n\r\n")
        .await;
    assert_eq!(
        (ok.status, ok.text()),
        (200, "ada".to_string()),
        "схема и имя заголовка без учёта регистра, пользователь виден обработчику"
    );
    let spoof = c
        .call("GET /me HTTP/1.1\r\nHost: t\r\nX-User: admin\r\nAuthorization: Bearer t-ada\r\n\r\n")
        .await;
    assert_eq!(
        spoof.text(),
        "ada",
        "X-User, пришедший от клиента, должен быть затёрт"
    );
    let public = c
        .call("GET /hello HTTP/1.1\r\nHost: t\r\nX-User: admin\r\n\r\n")
        .await;
    assert_eq!(
        (public.status, public.text()),
        (200, "hello".to_string()),
        "публичный путь открыт без токена"
    );
    let public_spoof = c
        .call("GET /me HTTP/1.1\r\nHost: t\r\nX-User: admin\r\n\r\n")
        .await;
    assert_eq!(public_spoof.status, 401);

    let lines = locked(&log).clone();
    assert!(
        lines.iter().all(|line| line.ends_with("ms")),
        "у каждой строки журнала время обработки в миллисекундах: {lines:?}"
    );
    let lines: Vec<&str> = lines
        .iter()
        .filter_map(|line| line.rsplit_once(' ').map(|(head, _)| head))
        .collect();
    assert_eq!(
        lines,
        vec![
            "GET /me 401",
            "GET /me 401",
            "GET /me 401",
            "GET /me 200",
            "GET /me 200",
            "GET /hello 200",
            "GET /me 401"
        ],
        "журнал снаружи авторизации видит и отказы"
    );
}

#[tokio::test]
async fn apply_puts_first_layer_outermost() {
    let trace = Arc::new(Mutex::new(Vec::<String>::new()));
    let layer = |name: &'static str, trace: Arc<Mutex<Vec<String>>>| {
        http_engine::middleware(move |next: Handler| {
            let trace = trace.clone();
            handler(move |request| {
                let (next, trace) = (next.clone(), trace.clone());
                async move {
                    locked(&trace).push(format!("{name} in"));
                    let response = next(request).await;
                    locked(&trace).push(format!("{name} out"));
                    response
                }
            })
        })
    };
    let inner = {
        let trace = trace.clone();
        handler(move |_| {
            let trace = trace.clone();
            async move {
                locked(&trace).push("handler".to_string());
                Response::text(200, "x")
            }
        })
    };
    let app = apply(
        inner,
        &[layer("a", trace.clone()), layer("b", trace.clone())],
    );
    app(Request::new(Method::Get, "/")).await;
    assert_eq!(
        *locked(&trace),
        ["a in", "b in", "handler", "b out", "a out"],
        "первый слой в списке самый внешний"
    );
}

#[tokio::test]
async fn connection_limit_holds_extra_clients_in_the_queue() {
    let config = ServerConfig {
        max_connections: 2,
        ..quiet()
    };
    let addr = start(base_router().into_handler(), config).await;
    let mut a = Client::connect(addr).await;
    let mut b = Client::connect(addr).await;
    assert_eq!(a.call(&get("/hello")).await.status, 200);
    assert_eq!(b.call(&get("/hello")).await.status, 200);
    let mut c = Client::connect(addr).await;
    c.send(&get("/hello")).await;
    let waited = timeout(Duration::from_millis(300), c.reply_for(false)).await;
    assert!(
        waited.is_err(),
        "третьего клиента сервер не должен обслуживать, пока заняты оба места"
    );
    drop(a);
    let reply = timeout(Duration::from_secs(3), c.reply())
        .await
        .expect("после освобождения места сервер должен принять третьего");
    assert_eq!(reply.text(), "hello");
}

#[tokio::test]
async fn graceful_shutdown_finishes_inflight_and_closes_idle() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен всегда");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let finished = Arc::new(AtomicBool::new(false));
    let marker = finished.clone();
    let server = tokio::spawn(async move {
        let server = Server::new(base_router()).config(quiet());
        let result = server
            .serve(listener, async {
                let _ = stop_rx.await;
            })
            .await;
        marker.store(true, Ordering::SeqCst);
        result
    });

    let mut idle = Client::connect(addr).await;
    assert_eq!(idle.call(&get("/hello")).await.status, 200);
    let mut busy = Client::connect(addr).await;
    busy.send(&get("/slow")).await;
    tokio::time::sleep(Duration::from_millis(60)).await;

    let _ = stop_tx.send(());
    idle.assert_closed(Duration::from_secs(1)).await;
    assert!(
        !finished.load(Ordering::SeqCst),
        "сервер не должен завершиться, пока обрабатывается запрос"
    );

    let reply = busy.reply().await;
    assert_eq!(
        (reply.status, reply.text()),
        (200, "slow".to_string()),
        "запрос в работе обязан получить полный ответ"
    );
    assert_eq!(
        reply.header("connection").as_deref(),
        Some("close"),
        "останавливаясь, сервер закрывает keep-alive"
    );
    busy.assert_closed(Duration::from_secs(1)).await;

    timeout(Duration::from_secs(2), server)
        .await
        .expect("serve должен завершиться, когда все запросы доделаны")
        .expect("задача сервера не падает")
        .expect("serve без ошибки");
    assert!(finished.load(Ordering::SeqCst));
    let late = TcpStream::connect(addr).await;
    if let Ok(mut late) = late {
        let mut buf = [0u8; 16];
        let _ = late.write_all(get("/hello").as_bytes()).await;
        let read = timeout(Duration::from_secs(1), late.read(&mut buf)).await;
        assert!(
            matches!(read, Ok(Ok(0)) | Ok(Err(_))),
            "после остановки новые клиенты не обслуживаются"
        );
    }
}

#[tokio::test]
async fn calls_counter_sanity() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let app = handler(move |_| {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Response::text(200, "ok")
        }
    });
    let addr = start(app, quiet()).await;
    let mut c = Client::connect(addr).await;
    c.send(&format!("{}{}{}", get("/a"), get("/b"), get("/c")))
        .await;
    for _ in 0..3 {
        assert_eq!(c.reply().await.status, 200);
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "каждый из трёх конвейерных запросов обработан ровно один раз"
    );
}
