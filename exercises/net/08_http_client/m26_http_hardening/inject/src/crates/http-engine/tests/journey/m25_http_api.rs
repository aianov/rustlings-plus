use std::future::Future;
use std::net::SocketAddr;
use std::panic::resume_unwind;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use http_engine::{
    Headers, Limits, Method, ParseError, Parsed, Request, Response, Router, Server, StatusCode,
    handler, parse_request, with_state,
};
use minifuzz::{Config, Rng, check_splits, fuzz, run_property};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn complete(raw: &[u8]) -> (Request, usize) {
    match parse_request(raw, &Limits::default()) {
        Ok(Parsed::Complete { request, consumed }) => (request, consumed),
        other => panic!("ждали целый запрос, а пришло {other:?}"),
    }
}

#[test]
fn parser_is_incremental_and_counts_bytes() {
    let raw = b"POST /a?x=1 HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\n\r\nhelloGET /next";
    for end in 0..raw.len() - 14 {
        assert_eq!(
            parse_request(&raw[..end], &Limits::default()),
            Ok(Parsed::Partial),
            "первые {end} байт это ещё не запрос"
        );
    }
    let (request, consumed) = complete(raw);
    assert_eq!(
        (
            request.method,
            request.raw_path(),
            &request.body[..],
            consumed
        ),
        (Method::Post, "/a", &b"hello"[..], raw.len() - 9),
        "тело ровно по Content-Length, остаток принадлежит следующему запросу"
    );
    assert_eq!(
        request.query(),
        Ok(vec![(String::from("x"), String::from("1"))])
    );
}

#[test]
fn parser_refuses_what_real_servers_got_burned_on() {
    let limits = Limits {
        max_head_bytes: 256,
        max_headers: 4,
        max_body: 16,
    };
    let cases: [(&[u8], ParseError); 7] = [
        (b"GET / HTTP/1.1\r\n\r\n", ParseError::MissingHost),
        (b"GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n", ParseError::DuplicateHost),
        (
            b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\nContent-Length: 4\r\n\r\n",
            ParseError::Smuggling,
        ),
        (
            b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n",
            ParseError::Smuggling,
        ),
        (b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 99\r\n\r\n", ParseError::BodyTooLarge),
        (b"GET / HTTP/1.1\r\nHost: a\r\nA: 1\r\nB: 2\r\nC: 3\r\nD: 4\r\n\r\n", ParseError::TooManyHeaders),
        (b"BREW / HTTP/1.1\r\nHost: a\r\n\r\n", ParseError::BadMethod),
    ];
    for (raw, expected) in cases {
        assert_eq!(
            parse_request(raw, &limits),
            Err(expected),
            "{:?}",
            String::from_utf8_lossy(raw)
        );
    }
    assert_eq!(
        parse_request(&[b'a'; 300], &limits),
        Err(ParseError::HeadTooLarge),
        "заголовки без конца длиннее лимита это 431, не ожидание"
    );
    assert_eq!(ParseError::HeadTooLarge.status(), 431);
    assert_eq!(ParseError::BodyTooLarge.status(), 413);
    assert_eq!(ParseError::Smuggling.status(), 400);
}

#[test]
fn chunked_bodies_and_escapes() {
    let (request, _) = complete(
        b"POST /c HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n5;ext=1\r\nhello\r\n6\r\n world\r\n0\r\nTrailer: x\r\n\r\n",
    );
    assert_eq!(
        request.body, b"hello world",
        "чанки склеиваются, расширения и трейлеры не мешают"
    );
    let (request, _) =
        complete(b"GET /users/%D0%B0%20b?q=a%26b%3D1&t=x+y HTTP/1.1\r\nHost: a\r\n\r\n");
    assert_eq!(
        request.path().as_deref(),
        Ok("/users/а b"),
        "percent-decoding пути в UTF-8"
    );
    assert_eq!(
        request.query(),
        Ok(vec![
            (String::from("q"), String::from("a&b=1")),
            (String::from("t"), String::from("x y"))
        ]),
        "query раскодируется после разбиения по & и =, + это пробел"
    );
}

#[test]
fn response_bytes_are_exact_and_safe() {
    let mut response = Response::text(200, "hi");
    assert_eq!(
        response.to_bytes(false),
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 2\r\n\r\nhi",
        "строка статуса, заголовки, Content-Length и тело"
    );
    assert!(
        !response.to_bytes(true).ends_with(b"hi"),
        "ответ на HEAD без тела, но с теми же заголовками"
    );
    assert_eq!(
        response.header("X-Name", "a\r\nSet-Cookie: evil=1"),
        Err(ParseError::BadHeader),
        "значение с \\r\\n дописало бы чужой заголовок: response splitting"
    );
    assert_eq!(
        StatusCode::new(404).map(StatusCode::reason),
        Some("Not Found")
    );
    assert_eq!(StatusCode::new(42), None, "кодов вне 100..=599 нет");
    let mut headers = Headers::new();
    headers.append("Set-Cookie", "a=1");
    headers.append("set-cookie", "b=2");
    assert_eq!(
        headers.get_all("SET-COOKIE"),
        ["a=1", "b=2"],
        "имена без учёта регистра, порядок сохранён"
    );
}

fn feed(chunks: &[&[u8]]) -> Result<Option<(Request, usize)>, ParseError> {
    let mut buf = Vec::new();
    let mut last = None;
    for chunk in chunks {
        buf.extend_from_slice(chunk);
        last = Some(parse_request(&buf, &Limits::default()));
        if let Some(Ok(Parsed::Complete { .. }) | Err(..)) = last {
            break;
        }
    }
    match last.unwrap_or(Ok(Parsed::Partial))? {
        Parsed::Complete { request, consumed } => Ok(Some((request, consumed))),
        Parsed::Partial => Ok(None),
    }
}

#[test]
fn any_split_of_the_stream_gives_the_same_request() {
    let samples: [&[u8]; 3] = [
        b"GET /a/b?c=d HTTP/1.1\r\nHost: x\r\nX-Long: 1234567890\r\n\r\n",
        b"POST /p HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\nbodyrest",
        b"POST /c HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
    ];
    for sample in samples {
        if let Err(failure) = check_splits(sample, feed) {
            panic!(
                "разрез в {:?} дал {:?}, а целый вход {:?}: парсер теряет состояние на границе",
                failure.cuts, failure.split, failure.whole
            );
        }
    }
}

fn random_request(rng: &mut Rng) -> Vec<u8> {
    let methods = ["GET", "POST", "PUT", "DELETE"];
    let method = rng.pick(&methods).copied().unwrap_or("GET");
    let segments = rng.range(1, 4);
    let mut path = String::new();
    for _ in 0..segments {
        path.push('/');
        for _ in 0..rng.range(1, 6) {
            path.push(char::from(b'a' + rng.byte() % 26));
        }
    }
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: test\r\n");
    for n in 0..rng.below(4) {
        raw.push_str(&format!("X-H{n}: v{}\r\n", rng.below(1000)));
    }
    let body: Vec<u8> = (0..rng.below(40)).map(|_| rng.byte()).collect();
    raw.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut bytes = raw.into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

#[test]
fn generated_requests_parse_back() {
    let checked = run_property(300, 42, |rng| {
        let raw = random_request(rng);
        match parse_request(&raw, &Limits::default()) {
            Ok(Parsed::Complete { consumed, .. }) if consumed == raw.len() => Ok(()),
            other => Err(format!("{:?} → {other:?}", String::from_utf8_lossy(&raw))),
        }
    });
    if let Err(failure) = checked {
        panic!("seed {}: {}", failure.seed, failure.message);
    }
}

#[test]
fn parser_never_panics_on_garbage() {
    let corpus = vec![
        b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(),
        b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\nhi".to_vec(),
        b"POST /c HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n"
            .to_vec(),
    ];
    let config = Config {
        seed: 7,
        max_iterations: 20_000,
        max_time: Duration::from_secs(20),
    };
    let crash = fuzz(&config, &corpus, |raw| {
        let _ = parse_request(raw, &Limits::default());
    });
    assert!(
        crash.is_none(),
        "фаззер нашёл панику парсера: {crash:?}; минимальный вход стоит сохранить отдельным тестом"
    );
}

#[test]
fn minifuzz_is_a_dev_only_dependency() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml http-engine читается");
    let normal = manifest
        .split("[dependencies]")
        .nth(1)
        .map(|rest| rest.split("\n[").next().unwrap_or_default())
        .unwrap_or_default();
    assert!(
        !normal.contains("minifuzz"),
        "minifuzz нужен только тестам: его место в [dev-dependencies], а не в [dependencies]"
    );
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Note {
    text: String,
}

struct State {
    hits: AtomicUsize,
}

async fn count(state: Arc<State>, request: Request) -> Response {
    let hits = state.hits.fetch_add(1, Ordering::SeqCst) + 1;
    Response::text(
        200,
        &format!("{} #{hits}", request.param("id").unwrap_or("?")),
    )
}

async fn echo(_: Arc<State>, request: Request) -> Response {
    match request.json::<Note>() {
        Ok(note) => Response::json(201, &note),
        Err(response) => response,
    }
}

fn explode() -> Response {
    resume_unwind(Box::new("обработчик упал"))
}

fn router() -> Router {
    let state = Arc::new(State {
        hits: AtomicUsize::new(0),
    });
    Router::new()
        .get("/users/:id", with_state(Arc::clone(&state), count))
        .get(
            "/users/me",
            handler(|_| async { Response::text(200, "это ты") }),
        )
        .post("/notes", with_state(state, echo))
        .get("/boom", handler(|_| async { explode() }))
}

struct Running {
    addr: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}

async fn start() -> Running {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен всегда");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    let (stop, stopped) = oneshot::channel::<()>();
    let server = Server::new(router());
    let task = tokio::spawn(server.serve(listener, async move {
        let _ = stopped.await;
    }));
    Running { addr, stop, task }
}

async fn send(addr: SocketAddr, raw: &str) -> String {
    let mut stream = within("подключение", TcpStream::connect(addr))
        .await
        .expect("сервер принимает соединения");
    let raw = raw.replacen("\r\n", "\r\nConnection: close\r\n", 1);
    stream
        .write_all(raw.as_bytes())
        .await
        .expect("запрос уходит");
    let mut answer = Vec::new();
    within("ответ", stream.read_to_end(&mut answer))
        .await
        .expect("ответ читается");
    String::from_utf8(answer).expect("ответ в UTF-8")
}

#[tokio::test]
async fn server_routes_json_head_and_panics() {
    let running = start().await;
    let addr = running.addr;
    let one = send(addr, "GET /users/7 HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        one.starts_with("HTTP/1.1 200 OK\r\n") && one.ends_with("7 #1"),
        "параметр пути и общий state: {one}"
    );
    let me = send(addr, "GET /users/me HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        me.ends_with("это ты"),
        "статичный сегмент побеждает параметр: {me}"
    );
    let head = send(addr, "HEAD /users/8 HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        head.starts_with("HTTP/1.1 200 OK\r\n")
            && head.contains("Content-Length: 4\r\n")
            && head.ends_with("\r\n\r\n"),
        "HEAD отвечает заголовками GET и без тела: {head:?}"
    );
    let missing = send(addr, "GET /nope HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        missing.starts_with("HTTP/1.1 404 "),
        "неизвестный путь 404: {missing}"
    );
    let wrong = send(addr, "DELETE /users/7 HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        wrong.starts_with("HTTP/1.1 405 ") && wrong.contains("Allow: GET, HEAD\r\n"),
        "путь есть, метода нет: 405 с Allow: {wrong}"
    );
    let body = r#"{"text":"привет"}"#;
    let created = send(
        addr,
        &format!("POST /notes HTTP/1.1\r\nHost: t\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n{body}", body.len()),
    )
    .await;
    assert!(
        created.starts_with("HTTP/1.1 201 ") && created.ends_with(body),
        "JSON на входе и выходе: {created}"
    );
    let plain = send(addr, "POST /notes HTTP/1.1\r\nHost: t\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nhi").await;
    assert!(
        plain.starts_with("HTTP/1.1 415 "),
        "не JSON это 415: {plain}"
    );
    let broken = send(addr, "POST /notes HTTP/1.1\r\nHost: t\r\nContent-Type: application/json\r\nContent-Length: 3\r\n\r\n{no").await;
    assert!(
        broken.starts_with("HTTP/1.1 400 "),
        "кривой JSON это 400: {broken}"
    );
    let boom = send(addr, "GET /boom HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        boom.starts_with("HTTP/1.1 500 "),
        "паника обработчика это 500: {boom}"
    );
    let after = send(addr, "GET /users/9 HTTP/1.1\r\nHost: t\r\n\r\n").await;
    assert!(
        after.ends_with("9 #3"),
        "после паники сервер живёт, state цел: {after}"
    );
    let garbage = send(addr, "HELLO\r\n\r\n").await;
    assert!(
        garbage.starts_with("HTTP/1.1 400 "),
        "мусор это 400: {garbage}"
    );
    let _ = running.stop.send(());
    within("остановка сервера", running.task)
        .await
        .expect("задача сервера не падает")
        .expect("serve завершается без ошибки");
}
