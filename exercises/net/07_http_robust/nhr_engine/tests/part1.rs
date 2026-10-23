use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use http_engine::{Method, Request, Response, Router, handler, serve};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

#[derive(Serialize, Deserialize)]
struct NewUser {
    name: String,
}

fn app() -> Router {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    Router::new()
        .route(Method::Get, "/health", handler(|_| async { Response::text(200, "ok") }))
        .route(Method::Get, "/users/:id", handler(|r: Request| async move {
            let id = r.params.get("id").unwrap_or("?").to_string();
            Response::json(200, &serde_json::json!({ "id": id }))
        }))
        .route(Method::Get, "/users/me", handler(|_| async { Response::text(200, "me") }))
        .route(Method::Get, "/files/:name", handler(|r: Request| async move {
            Response::text(200, r.params.get("name").unwrap_or("?"))
        }))
        .route(Method::Post, "/echo", handler(|r: Request| async move {
            match r.json::<serde_json::Value>() {
                Ok(v) => Response::json(200, &v),
                Err(response) => response,
            }
        }))
        .route(Method::Post, "/users", handler(|r: Request| async move {
            match r.json::<NewUser>() {
                Ok(u) => Response::json(201, &u),
                Err(response) => response,
            }
        }))
        .route(Method::Post, "/count", handler(move |_| {
            let hits = counter.clone();
            async move { Response::text(200, &(hits.fetch_add(1, Ordering::SeqCst) + 1).to_string()) }
        }))
        .route(Method::Get, "/count", handler(move |_| {
            let hits = hits.clone();
            async move { Response::text(200, &hits.load(Ordering::SeqCst).to_string()) }
        }))
        .route(Method::Get, "/boom", handler(|_| async {
            if true {
                panic!("boom");
            }
            Response::text(200, "unreachable")
        }))
        .route(Method::Get, "/slow", handler(|_| async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Response::text(200, "slow")
        }))
}

async fn start(router: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, router));
    addr
}

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

async fn read_reply(stream: &mut TcpStream) -> Reply {
    let mut raw = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut raw))
        .await
        .expect("сервер не закрыл соединение после ответа за 5 секунд, хотя в запросе стоит Connection: close")
        .expect("ошибка чтения");
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("в ответе нет пустой строки после заголовков");
    let head = String::from_utf8(raw[..split].to_vec()).expect("голова ответа должна быть UTF-8");
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok()).expect("в первой строке нет кода статуса");
    Reply { status, head, body: raw[split + 4..].to_vec() }
}

async fn send(addr: SocketAddr, request: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    read_reply(&mut stream).await
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
}

fn post(path: &str, content_type: Option<&str>, body: &str) -> String {
    let ct = content_type.map(|c| format!("Content-Type: {c}\r\n")).unwrap_or_default();
    format!("POST {path} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n{ct}Content-Length: {}\r\n\r\n{body}", body.len())
}

#[tokio::test]
async fn routes_404_and_405() {
    let addr = start(app()).await;
    let ok = send(addr, &get("/health")).await;
    assert_eq!(ok.status, 200);
    assert_eq!(ok.text(), "ok");
    assert_eq!(ok.header("content-length").as_deref(), Some("2"), "сервер сам ставит Content-Length");
    assert_eq!(ok.header("connection").as_deref(), Some("close"), "закрывая соединение, сервер должен об этом сказать");

    let missing = send(addr, &get("/nope")).await;
    assert_eq!(missing.status, 404);
    assert!(missing.text().contains("\"error\""), "404 тоже отвечает JSON с полем error: {}", missing.text());

    let wrong = send(addr, &post("/health", None, "")).await;
    assert_eq!(wrong.status, 405, "путь есть, метода нет: это 405, а не 404");
    assert_eq!(wrong.header("allow").as_deref(), Some("GET, HEAD"), "405 обязан перечислить допустимые методы (HEAD идёт вместе с GET)");
}

#[tokio::test]
async fn path_params_and_priority() {
    let addr = start(app()).await;
    let user = send(addr, &get("/users/7")).await;
    assert_eq!(user.text(), r#"{"id":"7"}"#);
    let me = send(addr, &get("/users/me")).await;
    assert_eq!(me.text(), "me", "статичный сегмент должен побеждать параметр");

    let reversed = Router::new()
        .route(Method::Get, "/users/me", handler(|_| async { Response::text(200, "me") }))
        .route(Method::Get, "/users/:id", handler(|_| async { Response::text(200, "param") }))
        .route(Method::Get, "/a/b/:y", handler(|_| async { Response::text(200, "a-b-y") }))
        .route(Method::Get, "/a/:x/c", handler(|_| async { Response::text(200, "a-x-c") }));
    let addr2 = start(reversed).await;
    assert_eq!(send(addr2, &get("/users/me")).await.text(), "me", "порядок регистрации не должен влиять на результат");
    assert_eq!(send(addr2, &get("/users/42")).await.text(), "param");
    assert_eq!(send(addr2, &get("/a/b/c")).await.text(), "a-b-y", "при равенстве побеждает маршрут, чей статичный сегмент левее");

    assert_eq!(send(addr, &get("/files/a%2Fb")).await.text(), "a/b", "параметр раскодируется после разбиения пути на сегменты");
    assert_eq!(send(addr, &get("/files/%ZZ")).await.status, 400);
    assert_eq!(send(addr, &get("/files/")).await.status, 404, "пустой параметр не совпадает");
    assert_eq!(send(addr, &get("/users/7/")).await.status, 404, "слеш в конце это другой путь");
    assert_eq!(send(addr, &get("/health?x=1")).await.status, 200, "query в маршрутизации не участвует");
}

#[tokio::test]
async fn json_in_and_out() {
    let addr = start(app()).await;
    let ok = send(addr, &post("/echo", Some("application/json"), r#"{"a":[1,2],"b":"é"}"#)).await;
    assert_eq!(ok.status, 200);
    assert_eq!(ok.header("content-type").as_deref(), Some("application/json"));
    assert_eq!(ok.text(), r#"{"a":[1,2],"b":"é"}"#);

    let charset = send(addr, &post("/echo", Some("Application/JSON; charset=UTF-8"), "{}")).await;
    assert_eq!(charset.status, 200, "регистр и charset=utf-8 допустимы");

    for ct in [Some("text/plain"), Some("application/jsonp"), Some("application/json; charset=latin1"), None] {
        let reply = send(addr, &post("/echo", ct, "{}")).await;
        assert_eq!(reply.status, 415, "Content-Type {ct:?} не подходит");
    }
    let bad = send(addr, &post("/echo", Some("application/json"), "{oops")).await;
    assert_eq!(bad.status, 400);
    assert!(bad.text().contains("error"), "тело 400 в формате {{\"error\": ...}}: {}", bad.text());
    let shape = send(addr, &post("/users", Some("application/json"), r#"{"name":5}"#)).await;
    assert_eq!(shape.status, 400, "правильный JSON не той формы это тоже 400");
    let created = send(addr, &post("/users", Some("application/json"), r#"{"name":"Ада"}"#)).await;
    assert_eq!(created.status, 201);
    assert_eq!(created.text(), r#"{"name":"Ада"}"#);
}

#[tokio::test]
async fn head_is_get_without_body() {
    let addr = start(app()).await;
    let full = send(addr, &get("/users/7")).await;
    let head = send(addr, "HEAD /users/7 HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").await;
    assert_eq!(head.status, 200, "HEAD должен находить GET-обработчик");
    assert!(head.body.is_empty(), "в ответе на HEAD тела быть не должно");
    assert_eq!(head.header("content-length"), full.header("content-length"), "Content-Length как у GET");
    assert_eq!(head.header("content-type"), full.header("content-type"));
    assert_eq!(send(addr, "HEAD /nope HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").await.status, 404);
}

#[tokio::test]
async fn panic_gives_500_and_server_lives() {
    let addr = start(app()).await;
    let boom = send(addr, &get("/boom")).await;
    assert_eq!(boom.status, 500);
    assert!(!boom.text().contains("boom"), "причину паники клиенту показывать нельзя");
    for _ in 0..3 {
        assert_eq!(send(addr, &get("/health")).await.status, 200, "после паники сервер должен жить");
    }
}

#[tokio::test]
async fn slow_handlers_do_not_block_each_other() {
    let addr = start(app()).await;
    let started = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..5 {
        tasks.push(tokio::spawn(async move { send(addr, &get("/slow")).await.status }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 200);
    }
    assert!(started.elapsed() < Duration::from_millis(1100), "пять запросов по 300 мс должны идти параллельно, ушло {:?}", started.elapsed());
}

#[tokio::test]
async fn request_in_tiny_pieces() {
    let addr = start(app()).await;
    let body = r#"{"name":"Борис"}"#;
    let raw = post("/users", Some("application/json"), body);
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.set_nodelay(true).unwrap();
    for piece in raw.as_bytes().chunks(3) {
        stream.write_all(piece).await.unwrap();
        stream.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let reply = read_reply(&mut stream).await;
    assert_eq!(reply.status, 201, "запрос, пришедший кусочками по 3 байта, должен собраться целиком");
    assert_eq!(reply.text(), format!(r#"{{"name":"Борис"}}"#));
}

#[tokio::test]
async fn limits_and_garbage() {
    let addr = start(app()).await;
    let mut huge = String::from("GET / HTTP/1.1\r\nHost: t\r\nConnection: close\r\n");
    while huge.len() < 100_000 {
        huge.push_str("X-Filler: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
    }
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let _ = stream.write_all(huge.as_bytes()).await;
    let reply = read_reply(&mut stream).await;
    assert_eq!(reply.status, 431, "огромная голова это 431");

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"POST /echo HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 99999999999\r\n\r\n")
        .await
        .unwrap();
    let reply = read_reply(&mut stream).await;
    assert_eq!(reply.status, 413, "слишком большая заявленная длина отвергается сразу, не дожидаясь тела");

    assert_eq!(send(addr, "GARBAGE\r\n\r\n").await.status, 400);
    assert_eq!(send(addr, "BREW / HTTP/1.1\r\nHost: t\r\n\r\n").await.status, 501);
    assert_eq!(send(addr, "GET / HTTP/2.0\r\nHost: t\r\n\r\n").await.status, 505);
    assert_eq!(send(addr, "GET / HTTP/1.1\r\n\r\n").await.status, 400, "в HTTP/1.1 Host обязателен");

    let mut silent = TcpStream::connect(addr).await.unwrap();
    silent.write_all(b"GET / HT").await.unwrap();
    drop(silent);
    assert_eq!(send(addr, &get("/health")).await.status, 200, "оборванный клиент сервер не ломает");
}

#[tokio::test]
async fn shared_state_across_connections() {
    let addr = start(app()).await;
    let mut tasks = Vec::new();
    for _ in 0..50 {
        tasks.push(tokio::spawn(async move { send(addr, &post("/count", None, "")).await.status }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 200);
    }
    assert_eq!(send(addr, &get("/count")).await.text(), "50", "состояние общее на все соединения");
}
