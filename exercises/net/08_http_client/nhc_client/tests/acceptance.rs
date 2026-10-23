use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http_client::{Client, Config, Error, Request};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await.ok()? == 0 {
            return None;
        }
        buf.push(byte[0]);
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

async fn read_body(stream: &mut TcpStream, head: &str) -> Vec<u8> {
    let len = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await.unwrap();
    body
}

fn spawn_server<F, Fut>(script: F) -> (SocketAddr, Arc<AtomicUsize>)
where
    F: Fn(usize, TcpStream) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    tokio::spawn(async move {
        let listener = TcpListener::from_std(listener).unwrap();
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let n = counter.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(script(n, stream));
        }
    });
    (addr, accepted)
}

type Seen = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

fn recording_server(respond: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static) -> (SocketAddr, Arc<AtomicUsize>, Seen) {
    let seen: Seen = Arc::default();
    let log = seen.clone();
    let respond = Arc::new(respond);
    let (addr, accepted) = spawn_server(move |_, mut stream| {
        let log = log.clone();
        let respond = respond.clone();
        async move {
            while let Some(head) = read_head(&mut stream).await {
                let body = read_body(&mut stream, &head).await;
                let wire = respond(&head);
                log.lock().unwrap().push((head, body));
                if stream.write_all(&wire).await.is_err() {
                    return;
                }
            }
        }
    });
    (addr, accepted, seen)
}

fn ok(body: &str) -> Vec<u8> {
    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn first_line(head: &str) -> &str {
    head.lines().next().unwrap_or("")
}

fn client() -> Client {
    Client::new(Config::default())
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct User {
    name: String,
    age: u32,
}

#[tokio::test]
async fn json_round_trip_with_proper_headers() {
    let (addr, _, seen) = recording_server(|head| {
        if first_line(head).starts_with("POST") {
            b"HTTP/1.1 201 Created\r\nContent-Length: 26\r\n\r\n{\"name\":\"Bob\",\"age\":41}   ".to_vec()
        } else {
            ok("{\"name\":\"Ada\",\"age\":36}")
        }
    });
    let c = client();
    let ada: User = c.get_json(&format!("http://{addr}/users/1")).await.expect("GET JSON");
    assert_eq!(ada, User { name: "Ada".into(), age: 36 });
    let bob: User = c.post_json(&format!("http://{addr}/users"), &User { name: "Bob".into(), age: 41 }).await.expect("POST JSON");
    assert_eq!(bob.name, "Bob");
    let seen = seen.lock().unwrap();
    let get = seen[0].0.to_ascii_lowercase();
    assert!(get.starts_with("get /users/1 http/1.1\r\n") && get.contains("accept: application/json\r\n"), "{get:?}");
    assert!(get.contains(&format!("host: {addr}\r\n")), "Host с портом: {get:?}");
    let post = seen[1].0.to_ascii_lowercase();
    assert!(post.contains("content-type: application/json\r\n") && post.contains(&format!("content-length: {}\r\n", seen[1].1.len())), "{post:?}");
    assert_eq!(serde_json::from_slice::<User>(&seen[1].1).unwrap(), User { name: "Bob".into(), age: 41 });
}

#[tokio::test]
async fn errors_are_told_apart() {
    let (addr, _, _) = recording_server(|head| match first_line(head) {
        l if l.contains("/missing") => b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\n\r\ngone".to_vec(),
        l if l.contains("/junk") => ok("<html>"),
        _ => ok("{}"),
    });
    let c = client();
    match c.get_json::<User>(&format!("http://{addr}/missing")).await {
        Err(Error::Status { code: 404, body }) => assert_eq!(body, "gone"),
        other => panic!("ожидали Status(404): {other:?}"),
    }
    assert!(matches!(c.get_json::<User>(&format!("http://{addr}/junk")).await, Err(Error::Decode(_))));
    assert!(matches!(c.get_json::<User>(&format!("http://{addr}/shape")).await, Err(Error::Decode(_))), "{{}} не User");
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    assert!(matches!(c.get_json::<User>(&format!("http://{closed}/")).await, Err(Error::Connect(_))));
    assert!(matches!(c.get("https://example.com/").await, Err(Error::Url(_))));
}

#[tokio::test]
async fn keep_alive_connection_is_reused() {
    let (addr, accepted, _) = recording_server(|_| ok("pong"));
    let c = client();
    for _ in 0..4 {
        assert_eq!(c.get(&format!("http://{addr}/ping")).await.unwrap().text(), "pong");
    }
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "все запросы к одному серверу по одному соединению");
    assert_eq!(c.idle_connections(), 1);
}

#[tokio::test]
async fn bodiless_answers_do_not_hang_and_keep_the_connection() {
    let (addr, accepted, _) = recording_server(|head| match first_line(head) {
        l if l.starts_with("HEAD") => b"HTTP/1.1 200 OK\r\nContent-Length: 5000\r\n\r\n".to_vec(),
        l if l.contains("/nc") => b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
        l if l.contains("/nm") => b"HTTP/1.1 304 Not Modified\r\nETag: \"v1\"\r\nContent-Length: 77\r\n\r\n".to_vec(),
        _ => b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndone".to_vec(),
    });
    let c = client();
    let started = Instant::now();
    let head = c.send(Request::new("HEAD", &format!("http://{addr}/big"))).await.unwrap();
    assert_eq!((head.status, head.body.len()), (200, 0), "HEAD без тела");
    assert_eq!(c.get(&format!("http://{addr}/nc")).await.unwrap().status, 204);
    let nm = c.get(&format!("http://{addr}/nm")).await.unwrap();
    assert_eq!((nm.status, nm.body.len(), nm.header("etag")), (304, 0, Some("\"v1\"")));
    let last = c.get(&format!("http://{addr}/after-100")).await.unwrap();
    assert_eq!((last.status, last.text()), (200, "done".to_string()), "100 Continue пропускается");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "после ответов без тела соединение по-прежнему годно");
}

#[tokio::test]
async fn chunked_and_close_delimited_bodies() {
    let (addr, accepted) = spawn_server(|_, mut stream| async move {
        while let Some(head) = read_head(&mut stream).await {
            if head.contains("/chunked") {
                let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n7;x=1\r\n, world\r\n0\r\nT: v\r\n\r\n";
                let _ = stream.write_all(wire).await;
            } else {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nuntil the end").await;
                return;
            }
        }
    });
    let c = client();
    assert_eq!(c.get(&format!("http://{addr}/chunked")).await.unwrap().text(), "Hello, world");
    assert_eq!(c.idle_connections(), 1, "chunked-ответ прочитан честно: соединение остаётся");
    assert_eq!(c.get(&format!("http://{addr}/close")).await.unwrap().text(), "until the end");
    assert_eq!(c.idle_connections(), 0, "ответ до закрытия в пул не попадает");
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "второй запрос шёл по тому же соединению, что и chunked");
}

#[tokio::test]
async fn garbage_and_oversized_answers_are_protocol_errors() {
    let (addr, accepted, _) = recording_server(|head| match first_line(head) {
        l if l.contains("/garbage") => b"NOT HTTP AT ALL\r\n\r\n".to_vec(),
        l if l.contains("/big") => format!("HTTP/1.1 200 OK\r\nContent-Length: 5000\r\n\r\n{}", "x".repeat(5000)).into_bytes(),
        _ => ok("fine"),
    });
    let c = Client::new(Config { max_body: 1000, ..Config::default() });
    assert!(matches!(c.get(&format!("http://{addr}/garbage")).await, Err(Error::Protocol(_))));
    assert!(matches!(c.get(&format!("http://{addr}/big")).await, Err(Error::Protocol(_))), "тело больше max_body");
    assert_eq!(c.get(&format!("http://{addr}/ok")).await.unwrap().text(), "fine", "после сбоя клиент работает дальше");
    assert!(accepted.load(Ordering::SeqCst) >= 3, "испорченные соединения не должны попасть обратно в пул");
}

#[tokio::test]
async fn redirects_follow_the_rules() {
    let (addr, _, seen) = recording_server(|head| {
        let line = first_line(head);
        let target = line.split(' ').nth(1).unwrap_or("");
        match target {
            "/a" => b"HTTP/1.1 302 Found\r\nLocation: /b\r\nContent-Length: 0\r\n\r\n".to_vec(),
            "/b" => b"HTTP/1.1 301 Moved Permanently\r\nLocation: c/d?x=1\r\nContent-Length: 0\r\n\r\n".to_vec(),
            "/c/d?x=1" => ok("arrived"),
            "/form" => b"HTTP/1.1 303 See Other\r\nLocation: /done\r\nContent-Length: 0\r\n\r\n".to_vec(),
            "/keep" => b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /kept\r\nContent-Length: 0\r\n\r\n".to_vec(),
            "/done" | "/kept" => ok(target),
            _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
        }
    });
    let c = client();
    let r = c.get(&format!("http://{addr}/a")).await.unwrap();
    assert_eq!((r.text(), r.redirects, r.url.path.as_str()), ("arrived".to_string(), 2, "/c/d?x=1"), "цепочка 302, 301 с относительным Location");
    let r = c.send(Request::new("POST", &format!("http://{addr}/form")).header("Content-Type", "text/plain").body("data")).await.unwrap();
    assert_eq!(r.text(), "/done");
    let r = c.send(Request::new("POST", &format!("http://{addr}/keep")).header("Content-Type", "text/plain").body("data")).await.unwrap();
    assert_eq!(r.text(), "/kept");
    let seen = seen.lock().unwrap();
    let done = seen.iter().find(|(h, _)| h.contains("/done")).unwrap();
    assert!(done.0.starts_with("GET "), "после 303 метод GET: {:?}", done.0);
    assert!(done.1.is_empty() && !done.0.to_ascii_lowercase().contains("content-type"), "тело и его заголовки не едут: {:?}", done.0);
    let kept = seen.iter().find(|(h, _)| h.contains("/kept")).unwrap();
    assert!(kept.0.starts_with("POST ") && kept.1 == b"data", "307 повторяет POST с телом");
}

#[tokio::test]
async fn credentials_stay_at_home() {
    let (b_addr, _, b_seen) = recording_server(|_| ok("other site"));
    let (a_addr, _, a_seen) = recording_server(move |head| {
        if head.contains("/go") {
            format!("HTTP/1.1 302 Found\r\nLocation: http://{b_addr}/landing\r\nContent-Length: 0\r\n\r\n").into_bytes()
        } else {
            ok("home")
        }
    });
    let c = client();
    let request = Request::get(&format!("http://{a_addr}/go")).header("Authorization", "Bearer s3cret").header("Cookie", "sid=1").header("Accept", "*/*");
    let r = c.send(request).await.unwrap();
    assert_eq!(r.text(), "other site");
    let a = a_seen.lock().unwrap()[0].0.to_ascii_lowercase();
    let b = b_seen.lock().unwrap()[0].0.to_ascii_lowercase();
    assert!(a.contains("authorization: bearer s3cret") && a.contains("cookie: sid=1"), "{a:?}");
    assert!(!b.contains("authorization") && !b.contains("cookie"), "на чужой хост учётные данные утекли: {b:?}");
    assert!(b.contains("accept: */*"), "остальные заголовки едут: {b:?}");
}

#[tokio::test]
async fn redirect_loops_stop() {
    let (addr, _, seen) = recording_server(|head| {
        let other = if head.contains("/ping") { "/pong" } else { "/ping" };
        format!("HTTP/1.1 302 Found\r\nLocation: {other}\r\nContent-Length: 0\r\n\r\n").into_bytes()
    });
    let c = Client::new(Config { max_redirects: 3, ..Config::default() });
    assert!(matches!(c.get(&format!("http://{addr}/ping")).await, Err(Error::TooManyRedirects)));
    assert_eq!(seen.lock().unwrap().len(), 4, "первый запрос и три перехода");
    let bad = recording_server(|_| b"HTTP/1.1 302 Found\r\nLocation: https://elsewhere.test/\r\nContent-Length: 0\r\n\r\n".to_vec()).0;
    assert!(matches!(c.get(&format!("http://{bad}/")).await, Err(Error::BadRedirect(_))));
}

#[tokio::test]
async fn total_deadline_covers_the_whole_request() {
    let (addr, _) = spawn_server(|_, mut stream| async move {
        let _ = read_head(&mut stream).await;
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n").await;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if stream.write_all(b"x").await.is_err() {
                return;
            }
        }
    });
    let c = Client::new(Config { total_timeout: Duration::from_millis(500), ..Config::default() });
    let started = Instant::now();
    let result = c.get(&format!("http://{addr}/slow")).await;
    assert!(matches!(result, Err(Error::TotalTimeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(3), "капающий сервер не должен держать клиента, прошло {:?}", started.elapsed());
}

#[tokio::test]
async fn stale_pool_connections_are_retried_for_safe_methods_only() {
    let requests = Arc::new(AtomicUsize::new(0));
    let r = requests.clone();
    let (addr, accepted) = spawn_server(move |n, mut stream| {
        let r = r.clone();
        async move {
            while let Some(head) = read_head(&mut stream).await {
                let _ = read_body(&mut stream, &head).await;
                r.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(&ok("ok")).await;
                if n == 0 {
                    return;
                }
            }
        }
    });
    let c = client();
    let url = format!("http://{addr}/x");
    assert_eq!(c.get(&url).await.unwrap().text(), "ok");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(c.get(&url).await.expect("GET на протухшем соединении повторяется").text(), "ok");
    assert_eq!(accepted.load(Ordering::SeqCst), 2);

    let (addr, accepted) = spawn_server(|n, mut stream| async move {
        while let Some(head) = read_head(&mut stream).await {
            let _ = read_body(&mut stream, &head).await;
            let _ = stream.write_all(&ok("ok")).await;
            if n == 0 {
                return;
            }
        }
    });
    let url = format!("http://{addr}/orders");
    let c = client();
    c.send(Request::new("POST", &url).body("{}")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(c.send(Request::new("POST", &url).body("{}")).await.is_err(), "POST на протухшем соединении не повторяется");
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_cli_prints_the_body_and_talks_in_verbose_mode() {
    let (addr, _, seen) = recording_server(|head| {
        if head.starts_with("POST") {
            b"HTTP/1.1 201 Created\r\nX-Made: yes\r\nContent-Length: 7\r\n\r\ncreated".to_vec()
        } else if head.contains("/missing") {
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\n\r\ngone".to_vec()
        } else {
            ok("hello from server")
        }
    });
    let bin = env!("CARGO_BIN_EXE_fetch");
    let run = |args: Vec<String>| async move { tokio::task::spawn_blocking(move || std::process::Command::new(bin).args(args).output().unwrap()).await.unwrap() };

    let out = run(vec![format!("http://{addr}/hi")]).await;
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello from server", "тело ответа в stdout без добавок");
    assert!(out.stderr.is_empty(), "без -v в stderr пусто: {:?}", String::from_utf8_lossy(&out.stderr));

    let out = run(vec!["-v".into(), "-H".into(), "X-Trace: abc".into(), "-d".into(), "payload".into(), format!("http://{addr}/make")]).await;
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{err}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "created");
    for line in ["> POST /make", &format!("> Host: {addr}"), "> X-Trace: abc", "< 201", "< X-Made: yes"] {
        assert!(err.lines().any(|l| l == line), "в stderr нет строки {line:?}: {err}");
    }
    let post = seen.lock().unwrap().iter().find(|(h, _)| h.starts_with("POST")).cloned().unwrap();
    assert_eq!(post.1, b"payload", "-d без -X это POST с телом");
    assert!(post.0.to_ascii_lowercase().contains("x-trace: abc"));

    let out = run(vec![format!("http://{addr}/missing")]).await;
    assert_eq!(out.status.code(), Some(1), "статус 4xx и 5xx это код выхода 1");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "gone");
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let out = run(vec![format!("http://{closed}/")]).await;
    assert_eq!(out.status.code(), Some(2), "не удалось подключиться: код 2");
    assert_eq!(run(vec![]).await.status.code(), Some(64), "без адреса это ошибка использования");
}
