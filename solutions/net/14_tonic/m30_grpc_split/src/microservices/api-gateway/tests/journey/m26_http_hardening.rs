use std::net::SocketAddr;
use std::time::Duration;

use api_gateway::Config;
use contracts::{UserView, decode};
use social_cli::Web;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::support::{Gateway, within};

struct Raw {
    status: u16,
    head: String,
    body: String,
}

impl Raw {
    fn header(&self, name: &str) -> Option<&str> {
        self.head.split("\r\n").skip(1).find_map(|line| {
            let (own, value) = line.split_once(':')?;
            own.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

async fn ask(addr: SocketAddr, path: &str, headers: &[(&str, &str)]) -> Raw {
    let mut stream = within("подключение к HTTP", TcpStream::connect(addr))
        .await
        .expect("HTTP-порт шлюза принимает соединения");
    let mut raw = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str("\r\n");
    stream
        .write_all(raw.as_bytes())
        .await
        .expect("запрос уходит");
    let mut answer = Vec::new();
    within("ответ HTTP", stream.read_to_end(&mut answer))
        .await
        .expect("ответ читается");
    let text = String::from_utf8(answer).expect("ответ в UTF-8");
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("кривая строка статуса: {head:?}"));
    Raw {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

#[tokio::test]
async fn me_needs_a_bearer_token() {
    let gateway = Gateway::start(Config::default()).await;
    let alice = gateway.register("alice").await;
    let bob = gateway.register("bob").await;
    let login = gateway.login("alice").await;
    let bearer = format!("Bearer {}", login.token);

    let anonymous = ask(gateway.http, "/users/me", &[]).await;
    assert_eq!(
        (anonymous.status, anonymous.header("www-authenticate")),
        (401, Some("Bearer")),
        "без токена /users/me это 401 с WWW-Authenticate: Bearer"
    );
    let stranger = ask(
        gateway.http,
        "/users/me",
        &[("Authorization", "Bearer 00000000000000000000000000000000")],
    )
    .await;
    assert_eq!(stranger.status, 401, "чужой токен это 401");
    let basic = ask(
        gateway.http,
        "/users/me",
        &[("Authorization", "Basic YWxpY2U6eA==")],
    )
    .await;
    assert_eq!(basic.status, 401, "схема только Bearer");
    let spoof = ask(
        gateway.http,
        "/users/me",
        &[("X-User", &bob.id.to_string())],
    )
    .await;
    assert_eq!(spoof.status, 401, "X-User от клиента ничего не значит");

    let me = ask(
        gateway.http,
        "/users/me",
        &[("authorization", &bearer), ("X-User", &bob.id.to_string())],
    )
    .await;
    assert_eq!(
        me.status, 200,
        "с токеном /users/me открывается: {}",
        me.body
    );
    let user: UserView = decode(&me.body)
        .unwrap_or_else(|e| panic!("ответ /users/me разбирается ({e}): {}", me.body));
    assert_eq!(
        user, alice,
        "/users/me это владелец токена, а не тот, кого клиент вписал в X-User"
    );
    let public = ask(gateway.http, "/users/bob", &[]).await;
    assert_eq!(
        public.status, 200,
        "остальные адреса по-прежнему открыты без токена"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn course_client_keeps_one_connection() {
    let gateway = Gateway::start(Config::default()).await;
    gateway.register("alice").await;
    let mut web = Web::new(&gateway.http.to_string());
    let calls = [
        ("GET", "/users/alice", None),
        ("GET", "/health", None),
        ("GET", "/users/nobody", None),
        ("GET", "/health", None),
        ("GET", "/posts/1", None),
        ("GET", "/users/alice", None),
    ];
    let mut statuses = Vec::new();
    for (method, path, body) in calls {
        let reply = within(
            &format!("{method} {path}"),
            web.request(method, path, body, None),
        )
        .await
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
        statuses.push(reply.status);
    }
    assert_eq!(
        statuses,
        [200, 200, 404, 200, 404, 200],
        "ответы по порядку"
    );
    assert_eq!(
        web.opened(),
        1,
        "шесть запросов по одному keep-alive соединению: сервер не закрывает его после ответа"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn http_client_pool_reuses_the_gateway_connection() {
    let gateway = Gateway::start(Config::default()).await;
    gateway.register("alice").await;
    let base = format!("http://{}", gateway.http);
    let client = http_client::Client::new(http_client::Config::default());
    for _ in 0..3 {
        let user: UserView = within(
            "GET /users/alice",
            client.get_json(&format!("{base}/users/alice")),
        )
        .await
        .unwrap_or_else(|e| panic!("профиль через http-client: {e}"));
        assert_eq!(user.username, "alice");
    }
    assert_eq!(
        client.idle_connections(),
        1,
        "пул клиента держит одно живое соединение со шлюзом"
    );
    match within("GET /users/me", client.get(&format!("{base}/users/me"))).await {
        Ok(response) => assert_eq!(response.status, 401, "без токена 401"),
        Err(error) => panic!("ответ 401 это ответ, а не ошибка транспорта: {error}"),
    }
    gateway.stop().await;
}

#[tokio::test]
async fn shutdown_closes_idle_keep_alive_connections() {
    let gateway = Gateway::start(Config::default()).await;
    let mut stream = TcpStream::connect(gateway.http)
        .await
        .expect("HTTP-порт открыт");
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: t\r\n\r\n")
        .await
        .expect("запрос уходит");
    let mut buf = vec![0u8; 4096];
    let n = within("ответ", stream.read(&mut buf))
        .await
        .expect("ответ читается");
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    assert!(
        head.starts_with("HTTP/1.1 200 ")
            && !head.to_ascii_lowercase().contains("connection: close"),
        "HTTP/1.1 по умолчанию keep-alive: {head:?}"
    );
    gateway.stop().await;
    let closed = timeout(Duration::from_secs(3), stream.read(&mut buf)).await;
    assert!(
        matches!(closed, Ok(Ok(0) | Err(_))),
        "остановка закрывает простаивающие keep-alive соединения"
    );
}

#[tokio::test]
async fn course_client_me_goes_with_bearer() {
    let gateway = Gateway::start(Config::default()).await;
    let mut screen = Vec::new();
    within(
        "сеанс клиента",
        social_cli::run(
            &gateway.http.to_string(),
            &gateway.auth.to_string(),
            "me\nregister alice\nlogin alice\nme\nquit\n".as_bytes(),
            &mut screen,
        ),
    )
    .await
    .expect("сеанс проходит");
    let screen = String::from_utf8(screen).expect("клиент пишет UTF-8");
    assert!(
        screen.contains("ты @alice (u_1)") && screen.contains("401"),
        "до входа me это отказ, после входа клиент сам кладёт Bearer-токен:\n{screen}"
    );
    gateway.stop().await;
}
