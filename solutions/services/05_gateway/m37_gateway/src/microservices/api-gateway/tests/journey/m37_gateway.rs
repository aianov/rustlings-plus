use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use api_gateway::{
    Config, Ctx, Duplicates, EXPIRED, GOING_AWAY, Principal, ProxyConfig, Routes, Services, report,
    run_with,
};
use auth_service::Lifetimes;
use contracts::{
    Changed, ErrorBody, Outcome, PostView, ProfileView, Response, StatsView, UserView, Value,
    decode,
};
use service_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, duplex};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::fakes::{fake_auth, fake_chat, fake_users, slow_posts, slow_users};
use crate::support::{DEADLINE, Gateway, Peer, id_of, listener, ok, within};

const DEAD: &str = "127.0.0.1:9";

fn quick(access: u64) -> auth_service::Config {
    auth_service::Config {
        lifetimes: Lifetimes {
            access,
            leeway: 0,
            ..Lifetimes::default()
        },
        ..auth_service::Config::default()
    }
}

fn changed() -> Changed {
    Changed { changed: true }
}

fn naps() -> Routes {
    Routes::social()
        .route_authed(
            "debug.nap",
            |_: Ctx, _: Principal, data: Value| async move {
                let ms = data.get("ms").and_then(Value::as_u64).unwrap_or(300);
                sleep(Duration::from_millis(ms)).await;
                Ok(changed())
            },
        )
        .route_authed(
            "debug.boom",
            |_: Ctx, _: Principal, data: Value| async move {
                if data.get("safe").is_none() {
                    panic!("обработчик падает нарочно");
                }
                Ok(changed())
            },
        )
}

fn services(auth: String, users: String, posts: String, chat: String) -> Services {
    Services {
        auth,
        auth_http: DEAD.to_string(),
        users,
        posts,
        chat,
    }
}

struct Lone {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<api_gateway::Result<()>>,
}

impl Lone {
    async fn start(config: Config) -> Lone {
        let (http, addr) = listener().await;
        let stop = CancellationToken::new();
        let task = tokio::spawn(run_with(http, config, Routes::social(), stop.clone()));
        Lone { addr, stop, task }
    }

    async fn peer(&self, path: &str) -> Peer {
        Peer::connect(self.addr, path).await
    }

    async fn stop(self) {
        self.stop.cancel();
        let finished = within("остановка шлюза", self.task)
            .await
            .expect("задача шлюза не падает");
        assert!(
            finished.is_ok(),
            "шлюз останавливается без ошибки: {finished:?}"
        );
    }
}

struct Raw {
    status: u16,
    head: String,
    body: String,
}

impl Raw {
    fn headers(&self, name: &str) -> Vec<&str> {
        self.head
            .split("\r\n")
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .filter(|(own, _)| own.trim().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim())
            .collect()
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers(name).into_iter().next()
    }

    fn error(&self) -> ErrorBody {
        match decode::<Outcome<()>>(&self.body) {
            Ok(Outcome::Error(body)) => body,
            other => panic!("ждали конверт ошибки: {other:?}, {}", self.body),
        }
    }
}

async fn exchange(addr: SocketAddr, request: &str) -> Raw {
    let mut stream = within("подключение", TcpStream::connect(addr))
        .await
        .expect("порт шлюза открыт");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("запрос уходит");
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        let n = within("ответ", stream.read(&mut chunk))
            .await
            .expect("ответ читается");
        assert!(n > 0, "шлюз закрыл соединение до конца заголовков");
        raw.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let length: usize = head
        .split("\r\n")
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    while raw.len() < head_end + 4 + length {
        let n = within("тело ответа", stream.read(&mut chunk))
            .await
            .expect("тело читается");
        assert!(n > 0, "шлюз закрыл соединение посреди тела");
        raw.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&raw[head_end + 4..head_end + 4 + length]).to_string();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("кривая строка статуса: {head:?}"));
    Raw { status, head, body }
}

fn upgrade(path: &str, extra: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n"
    )
}

#[tokio::test]
async fn duplicate_routes_stop_the_start_and_name_every_pair() {
    let router = Routes::social()
        .build()
        .expect("в таблице соцсети повторов нет");
    assert_eq!(
        router.is_authed("users.profile"),
        Some(false),
        "карточку профиля видит и гость"
    );
    let twice = || {
        Routes::new()
            .route("debug.a", |_: Ctx, _: Value| async { Ok(changed()) })
            .route_authed("debug.b", |_: Ctx, _: Principal, _: Value| async {
                Ok(changed())
            })
            .route("debug.a", |_: Ctx, _: Value| async { Ok(changed()) })
            .route("debug.b", |_: Ctx, _: Value| async { Ok(changed()) })
            .route("debug.a", |_: Ctx, _: Value| async { Ok(changed()) })
    };
    match twice().build() {
        Err(Duplicates(names)) => assert_eq!(
            names,
            ["debug.a", "debug.b"],
            "build называет каждую задвоенную пару один раз, в порядке появления"
        ),
        Ok(..) => panic!("таблица с повторами собралась"),
    }
    let (http, _) = listener().await;
    let started = within(
        "старт с повторами",
        run_with(http, Config::default(), twice(), CancellationToken::new()),
    )
    .await;
    let error = started.expect_err("шлюз с повторами в таблице не стартует");
    let text = report(&error);
    assert!(
        text.contains("debug.a") && text.contains("debug.b"),
        "ошибка старта называет задвоенные маршруты: {text}"
    );
}

#[tokio::test]
async fn bad_data_names_the_field_and_never_echoes_it() {
    let stack = Gateway::start(Config::default()).await;
    let (_, mut alice) = stack.member("alice").await;
    let wrong = alice.fails("posts.create", r#"{"text": 4242}"#).await;
    assert_eq!(wrong.code, "bad_data", "{wrong:?}");
    assert!(
        !format!("{wrong:?}").contains("4242") && !wrong.message.contains("integer"),
        "текст serde цитирует присланное и в ответ не идёт: {wrong:?}"
    );
    let missing = alice.fails("posts.create", r#"{"tags": ["rust"]}"#).await;
    assert_eq!(
        (missing.code.as_str(), missing.fields()),
        ("bad_data", vec![("text", "missing")]),
        "нехватку поля назвать можно: это имя из схемы, а не данные клиента"
    );
    let odd = alice.fails("posts.like", r#"{"post": "секрет-17"}"#).await;
    assert!(
        odd.code == "bad_data" && !format!("{odd:?}").contains("секрет"),
        "{odd:?}"
    );
    assert!(odd.request_id.is_some(), "у отказа есть номер запроса");
    let unknown = alice.fails("posts.dance", "{}").await;
    assert_eq!(unknown.code, "unknown_method");
    stack.stop().await;
}

#[tokio::test]
async fn guests_read_and_only_members_act() {
    let stack = Gateway::start(Config::default()).await;
    let alice = stack.register("alice").await;
    let mut guest = Peer::connect(stack.http, "/ws").await;
    let found = ok!(guest, "users.find", r#"{"username": "alice"}"# => UserView);
    assert_eq!(found.id, alice.id, "гость ищет пользователей");
    let card =
        ok!(guest, "users.profile", format!(r#"{{"target": "{}"}}"#, alice.id) => ProfileView);
    assert_eq!(card.user.username, "alice", "и смотрит карточки");
    let early = guest.fails("posts.create", r#""даже не объект""#).await;
    assert_eq!(
        early.code, "unauthorized",
        "вход проверяется до разбора data: гость не изучит схему по ошибкам, {early:?}"
    );
    let login = stack.login("alice").await;
    let header = format!("Authorization: Bearer {}\r\n", login.token);
    let same = exchange(
        stack.http,
        &upgrade(&format!("/ws?token={}", login.token), &header),
    )
    .await;
    assert_eq!(same.status, 101, "один и тот же токен дважды не спор");
    let other = exchange(stack.http, &upgrade("/ws?token=another", &header)).await;
    assert_eq!(
        (other.status, other.error().code.as_str()),
        (400, "bad_data"),
        "два разных токена: непонятно, кто пришёл, сокет не открывается"
    );
    stack.stop().await;
}

#[tokio::test]
async fn an_expired_token_closes_the_socket_after_started_work() {
    let stack = Gateway::custom(Config::default(), naps(), quick(2)).await;
    stack.register("alice").await;
    let login = stack.login("alice").await;
    let mut peer = stack.peer_with(&login.token).await;
    let began = Instant::now();
    let slow = peer.send("debug.nap", r#"{"ms": 1500}"#).await;
    let first = peer.text().await.expect("ответ на начатый запрос");
    assert!(
        id_of(&first) == Some(slow) && first.contains(r#""ok""#),
        "начатый запрос доделан и отвечен до закрытия: {first}"
    );
    let notice = peer.text().await.expect("уведомление о сроке");
    assert!(
        notice.contains(r#""id":null"#) && notice.contains(r#""code":"expired""#),
        "по сроку шлюз говорит expired без id: {notice}"
    );
    assert!(peer.closed().await, "после уведомления сокет закрыт");
    assert_eq!(
        peer.close_code(),
        Some(EXPIRED),
        "код закрытия 4001: обнови токен"
    );
    assert!(
        began.elapsed() < Duration::from_secs(4),
        "закрытие по сроку токена, а не по idle: {:?}",
        began.elapsed()
    );
    let again = exchange(
        stack.http,
        &upgrade(&format!("/ws?token={}", login.token), ""),
    )
    .await;
    assert_eq!(
        (again.status, again.error().code.as_str()),
        (401, "expired"),
        "с просроченным токеном сокет не открывается, и код говорит, что делать"
    );
    stack.stop().await;
}

#[tokio::test]
async fn in_flight_limit_refuses_at_once_and_survives_panics() {
    let stack = Gateway::start_with(
        Config {
            max_in_flight: 2,
            ..Config::default()
        },
        naps(),
    )
    .await;
    let (_, mut alice) = stack.member("alice").await;
    let began = Instant::now();
    let first = alice.send("debug.nap", "{}").await;
    let second = alice.send("debug.nap", "{}").await;
    let third = alice.send("debug.nap", "{}").await;
    let busy = alice.any_reply().await;
    assert!(
        began.elapsed() < Duration::from_millis(200),
        "отказ по лимиту без ожидания: {:?}",
        began.elapsed()
    );
    let refused: Response<Changed> = decode(&busy).expect("ответ разбирается");
    match refused {
        Response {
            id: Some(id),
            outcome: Outcome::Error(body),
            request_id: Some(..),
        } => assert_eq!(
            (id, body.code.as_str()),
            (third, "exhausted"),
            "третий запрос сверх лимита 2 получает exhausted со своим id"
        ),
        other => panic!("ждали отказ exhausted, а пришло {other:?}"),
    }
    let mut done = vec![
        id_of(&alice.any_reply().await),
        id_of(&alice.any_reply().await),
    ];
    done.sort_unstable();
    assert_eq!(done, [Some(first), Some(second)], "два в работе отвечены");
    let boom = alice.fails("debug.boom", "{}").await;
    assert!(
        boom.code == "internal" && boom.request_id.is_some(),
        "паника это internal с номером запроса: {boom:?}"
    );
    let a = alice.send("debug.nap", r#"{"ms": 50}"#).await;
    let b = alice.send("debug.nap", r#"{"ms": 50}"#).await;
    let mut after = vec![alice.reply(a).await, alice.reply(b).await];
    after.retain(|raw| raw.contains(r#""ok""#));
    assert_eq!(
        after.len(),
        2,
        "разрешение упавшей задачи вернулось: снова два места"
    );
    stack.stop().await;
}

#[tokio::test]
async fn shutdown_answers_what_was_started() {
    let stack = Gateway::start_with(Config::default(), naps()).await;
    let (_, mut alice) = stack.member("alice").await;
    let id = alice.send("debug.nap", r#"{"ms": 400}"#).await;
    sleep(Duration::from_millis(50)).await;
    let stopping = tokio::spawn(stack.stop());
    let reply = alice.reply(id).await;
    assert!(
        reply.contains(r#""ok""#),
        "при остановке начатый запрос доделан и отвечен: {reply}"
    );
    assert!(alice.closed().await);
    assert_eq!(alice.close_code(), Some(GOING_AWAY));
    within("остановка стека", stopping)
        .await
        .expect("остановка не падает");
}

#[tokio::test]
async fn metadata_comes_from_the_checked_context() {
    let stop = CancellationToken::new();
    let auth = fake_auth(&stop).await;
    let (users, calls) = fake_users(&stop).await;
    let (chat, _) = fake_chat(&stop).await;
    let gateway = Lone::start(Config {
        services: services(
            auth.to_string(),
            users.to_string(),
            DEAD.to_string(),
            chat.to_string(),
        ),
        ..Config::default()
    })
    .await;
    let mut alice = gateway.peer("/ws?token=alice-token").await;
    alice
        .raw(r#"{"id": 1, "service": "users", "method": "users.find", "data": {"username": "bob"}, "meta": {"x-user-id": "2", "X-User-Role": "admin", "x-request-id": "forged", "x-internal-debug": "1", "X-Trace-Id": "trace-7", "accept-language": "ru-RU"}}"#)
        .await;
    let reply: Response<UserView> = decode(&alice.reply(1).await).expect("ответ разбирается");
    let seen = calls.take_seen();
    let find = seen
        .iter()
        .find(|seen| seen.method == "FindProfile")
        .expect("шлюз спросил user-service");
    assert_eq!(
        (find.user.as_deref(), find.role.as_deref()),
        (Some("1"), Some("user")),
        "x-user-id и x-user-role берутся из токена, присланное клиентом выброшено"
    );
    assert!(
        find.request_id != "forged" && Some(&find.request_id) == reply.request_id.as_ref(),
        "x-request-id ставит шлюз, и он же в ответе клиенту: {find:?} {:?}",
        reply.request_id
    );
    assert_eq!(
        find.passed,
        [
            (String::from("x-trace-id"), String::from("trace-7")),
            (String::from("accept-language"), String::from("ru-RU"))
        ],
        "из клиентских metadata проходит только белый список, ключи в нижнем регистре"
    );
    assert!(
        find.internal.is_empty(),
        "x-internal-* клиента не проходит: {find:?}"
    );
    let long = "x".repeat(200);
    alice
        .raw(&format!(r#"{{"id": 2, "service": "users", "method": "users.find", "data": {{"username": "bob"}}, "meta": {{"x-trace-id": "a\nb", "accept-language": "{long}"}}}}"#))
        .await;
    let _ = alice.reply(2).await;
    let sloppy = calls.take_seen();
    assert!(
        sloppy.iter().all(|seen| seen.passed.is_empty()),
        "перевод строки и значение длиннее 128 байт не проходят: {sloppy:?}"
    );
    let mut guest = gateway.peer("/ws").await;
    let _ = ok!(guest, "users.find", r#"{"username": "bob"}"# => UserView);
    let anonymous = calls.take_seen();
    assert!(
        anonymous
            .iter()
            .all(|seen| seen.user.is_none() && seen.role.as_deref() == Some("guest")),
        "гость едет без x-user-id и с ролью guest: {anonymous:?}"
    );
    gateway.stop().await;
    stop.cancel();
}

#[tokio::test]
async fn the_profile_card_is_assembled_from_three_calls() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let target = format!(r#"{{"target": "{}"}}"#, alice.id);
    let _ = ok!(reader, "users.follow", &target => Changed);
    for text in ["первый", "второй"] {
        let _ = ok!(writer, "posts.create", format!(r#"{{"text": "{text}"}}"#) => PostView);
    }
    let card = ok!(reader, "users.profile", &target => ProfileView);
    assert_eq!(card.user.id, alice.id);
    let texts: Vec<String> = card
        .posts
        .iter()
        .flatten()
        .map(|post| post.text.clone())
        .collect();
    assert_eq!(
        texts,
        ["второй", "первый"],
        "последние посты автора, новые первыми"
    );
    assert_eq!(
        card.stats,
        Some(StatsView {
            followers: 1,
            following: 0
        })
    );
    assert!(card.partial.is_empty(), "все части пришли: {card:?}");
    stack.stop().await;
}

async fn card_stack(users_delay: u64, posts_delay: u64, parts: u64) -> (Lone, CancellationToken) {
    let stop = CancellationToken::new();
    let auth = fake_auth(&stop).await;
    let (users, _) = slow_users(&stop, Duration::from_millis(users_delay)).await;
    let (posts, _) = slow_posts(&stop, Duration::from_millis(posts_delay)).await;
    let (chat, _) = fake_chat(&stop).await;
    let gateway = Lone::start(Config {
        services: services(
            auth.to_string(),
            users.to_string(),
            posts.to_string(),
            chat.to_string(),
        ),
        part_budget: Duration::from_millis(parts),
        budget: Duration::from_secs(2),
        ..Config::default()
    })
    .await;
    (gateway, stop)
}

#[tokio::test]
async fn card_parts_run_together_and_late_parts_become_null() {
    let (gateway, stop) = card_stack(200, 200, 400).await;
    let mut guest = gateway.peer("/ws").await;
    let _ = ok!(guest, "users.find", r#"{"username": "alice"}"# => UserView);
    let began = Instant::now();
    let card = ok!(guest, "users.profile", r#"{"target": "u_1"}"# => ProfileView);
    let took = began.elapsed();
    assert!(
        card.partial.is_empty() && card.posts.as_ref().map(Vec::len) == Some(2),
        "{card:?}"
    );
    assert!(
        took < Duration::from_millis(350),
        "профиль и посты по 200 мс идут одновременно, а заняло {took:?}"
    );
    gateway.stop().await;
    stop.cancel();

    let (gateway, stop) = card_stack(0, 600, 150).await;
    let mut guest = gateway.peer("/ws").await;
    let _ = ok!(guest, "users.find", r#"{"username": "alice"}"# => UserView);
    let began = Instant::now();
    let card = ok!(guest, "users.profile", r#"{"target": "u_1"}"# => ProfileView);
    let took = began.elapsed();
    assert_eq!(
        (card.posts.is_none(), card.partial.clone()),
        (true, vec![String::from("posts")]),
        "посты не уложились в бюджет части: null и имя в partial"
    );
    assert!(card.stats.is_some(), "остальное пришло: {card:?}");
    assert!(
        took < Duration::from_millis(450),
        "зависший сосед не держит ответ дольше бюджета части: {took:?}"
    );
    let missing = guest.fails("users.profile", r#"{"target": "u_9"}"#).await;
    assert_eq!(
        missing.code, "not_found",
        "без обязательной части (пользователя) ответа нет"
    );
    gateway.stop().await;
    stop.cancel();
}

type Heads = Arc<Mutex<Vec<String>>>;

fn heads(seen: &Heads) -> Vec<String> {
    seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

async fn fake_auth_http() -> (SocketAddr, Heads) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    let seen: Heads = Arc::default();
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            let record = Arc::clone(&record);
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut chunk = [0u8; 4096];
                while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tcp.read(&mut chunk).await {
                        Ok(0) | Err(..) => return,
                        Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    }
                }
                let text = String::from_utf8_lossy(&raw).to_string();
                let head = text
                    .split("\r\n\r\n")
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let length: usize = head
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse().ok())
                    .unwrap_or(0);
                let body_at = head.len() + 4;
                while raw.len() < body_at + length {
                    match tcp.read(&mut chunk).await {
                        Ok(0) | Err(..) => return,
                        Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    }
                }
                let body = String::from_utf8_lossy(&raw[body_at..]).to_string();
                record
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(format!("{head}\r\n\r\n{body}"));
                if head.starts_with("POST /auth/slow") {
                    sleep(Duration::from_secs(2)).await;
                }
                let answer = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close, X-Hop\r\nX-Hop: hop-secret\r\nKeep-Alive: timeout=5\r\nSet-Cookie: session=s1; Path=/; HttpOnly\r\nSet-Cookie: refresh=r1; Path=/auth; HttpOnly\r\nContent-Length: 11\r\n\r\n{\"ok\":true}";
                let _ = tcp.write_all(answer.as_bytes()).await;
            });
        }
    });
    (addr, seen)
}

fn header_of<'a>(head: &'a str, name: &str) -> Vec<&'a str> {
    head.split("\r\n")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .filter(|(own, _)| own.trim().eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
        .collect()
}

async fn proxy_stack(auth_http: &str) -> Lone {
    Lone::start(Config {
        services: Services {
            auth_http: auth_http.to_string(),
            ..services(
                DEAD.to_string(),
                DEAD.to_string(),
                DEAD.to_string(),
                DEAD.to_string(),
            )
        },
        proxy: ProxyConfig {
            max_body: 1024,
            timeout: Duration::from_millis(400),
        },
        ..Config::default()
    })
    .await
}

#[tokio::test]
async fn the_auth_proxy_cleans_both_directions() {
    let (upstream, seen) = fake_auth_http().await;
    let gateway = proxy_stack(&upstream.to_string()).await;
    let body = r#"{"username":"alice","password":"correct-horse"}"#;
    let request = format!(
        "POST /auth/login?next=1 HTTP/1.1\r\nHost: social.test\r\nConnection: close, X-Drop-Me\r\nX-Drop-Me: 1\r\nKeep-Alive: timeout=9\r\nTE: trailers\r\nProxy-Authorization: Basic eA==\r\nX-Forwarded-For: 6.6.6.6\r\nX-Real-IP: 6.6.6.6\r\nForwarded: for=6.6.6.6\r\nX-Forwarded-Host: evil.test\r\nX-User-Id: 1\r\nX-User-Role: admin\r\nX-Internal-Debug: 1\r\nX-Request-Id: forged\r\nCookie: refresh=r0\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let answer = exchange(gateway.addr, &request).await;
    assert_eq!(answer.status, 200, "{}", answer.body);
    let recorded = heads(&seen);
    let [forwarded] = recorded.as_slice() else {
        panic!("auth-service получил ровно один запрос: {recorded:?}");
    };
    assert!(
        forwarded.starts_with("POST /auth/login?next=1 HTTP/1.1\r\n"),
        "путь и строка запроса доезжают как есть: {forwarded}"
    );
    for gone in [
        "x-drop-me",
        "keep-alive",
        "te",
        "proxy-authorization",
        "forwarded",
        "x-forwarded-host",
        "x-user-id",
        "x-user-role",
        "x-internal-debug",
    ] {
        assert!(
            header_of(forwarded, gone).is_empty(),
            "{gone} не уходит в auth-service: {forwarded}"
        );
    }
    assert_eq!(header_of(forwarded, "x-forwarded-for"), ["127.0.0.1"]);
    assert_eq!(header_of(forwarded, "x-real-ip"), ["127.0.0.1"]);
    assert_eq!(header_of(forwarded, "x-forwarded-proto"), ["http"]);
    assert_eq!(header_of(forwarded, "cookie"), ["refresh=r0"]);
    assert_eq!(
        header_of(forwarded, "x-request-id"),
        [answer
            .header("x-request-id")
            .expect("номер запроса в ответе")],
        "auth-service получает номер шлюза, а не присланный клиентом"
    );
    assert!(
        forwarded.ends_with(body),
        "тело доезжает целиком: {forwarded}"
    );
    assert_eq!(
        answer.headers("set-cookie").len(),
        2,
        "обе cookie входа доходят до клиента: {}",
        answer.head
    );
    assert!(
        answer.header("x-hop").is_none() && answer.header("keep-alive").is_none(),
        "hop-by-hop ответа срезаны: {}",
        answer.head
    );
    assert_eq!(answer.body, r#"{"ok":true}"#);

    for path in ["/auth/../users/me", "/auth/%2e%2e/users/me", "/auth/"] {
        let refused = exchange(
            gateway.addr,
            &format!("GET {path} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n"),
        )
        .await;
        assert_eq!(
            (refused.status, refused.error().code.as_str()),
            (404, "not_found"),
            "{path}: чужой путь отказан до апстрима"
        );
    }
    let big = "я".repeat(600);
    let too_big = exchange(
        gateway.addr,
        &format!(
            "POST /auth/register HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{big}",
            big.len()
        ),
    )
    .await;
    assert_eq!(
        (too_big.status, too_big.error().code.as_str()),
        (413, "too_large")
    );
    assert_eq!(
        heads(&seen).len(),
        1,
        "404 и 413 решены без апстрима: auth-service защищён от мусора"
    );
    let began = Instant::now();
    let late = exchange(
        gateway.addr,
        "POST /auth/slow HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    assert_eq!((late.status, late.error().code.as_str()), (504, "timeout"));
    assert!(
        began.elapsed() < Duration::from_millis(1500),
        "504 по таймауту прокси, а не по ответу апстрима: {:?}",
        began.elapsed()
    );
    gateway.stop().await;

    let gateway = proxy_stack(DEAD).await;
    let down = exchange(
        gateway.addr,
        "POST /auth/login HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    assert_eq!(
        (down.status, down.error().code.as_str()),
        (502, "unavailable"),
        "апстрим недоступен: 502, тело пишет шлюз"
    );
    assert!(
        !down.body.contains(DEAD),
        "адрес внутренней сети наружу не уходит: {}",
        down.body
    );
    gateway.stop().await;
}

#[tokio::test]
async fn the_course_client_needs_one_address() {
    let stack = Gateway::start(Config::default()).await;
    let mut screen = Vec::new();
    within(
        "сеанс клиента",
        social_cli::run_with(
            &stack.http.to_string(),
            stack.dialer.clone(),
            "find nobody\npost рано\nregister alice correct-horse\nlogin alice correct-horse\npost первый\npost второй\ncard alice\nquit\n".as_bytes(),
            &mut screen,
        ),
    )
    .await
    .expect("сеанс проходит");
    let screen = String::from_utf8(screen).expect("UTF-8");
    for needle in [
        "на связи, вход и регистрация через него же",
        "ошибка (not_found)",
        "сначала войди: login <ник> <пароль>",
        "зарегистрирован @alice (u_1)",
        "карточка @alice (u_1): подписчиков 0, подписок 0",
        "  @alice [2] второй",
        "  @alice [1] первый",
    ] {
        assert!(screen.contains(needle), "нет «{needle}» в\n{screen}");
    }
    stack.stop().await;
}

#[tokio::test]
async fn the_course_client_renews_when_the_socket_expires() {
    let stack = Gateway::with_auth(Config::default(), quick(2)).await;
    let (mut keys, typed) = duplex(4096);
    let (shown, mut glass) = duplex(64 * 1024);
    let (http, dialer) = (stack.http.to_string(), stack.dialer.clone());
    let session = tokio::spawn(async move {
        let _ = social_cli::run_with(&http, dialer, BufReader::new(typed), shown).await;
    });
    let screen = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&screen);
    let reader = tokio::spawn(async move {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = glass.read(&mut chunk).await {
            if n == 0 {
                break;
            }
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_str(&String::from_utf8_lossy(&chunk[..n]));
        }
    });
    let wait = |needle: &'static str| {
        let screen = Arc::clone(&screen);
        async move {
            let began = Instant::now();
            loop {
                let now = screen
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                if now.contains(needle) {
                    return;
                }
                assert!(began.elapsed() < DEADLINE, "нет «{needle}» в\n{now}");
                sleep(Duration::from_millis(20)).await;
            }
        }
    };
    keys.write_all(b"register alice correct-horse\nlogin alice correct-horse\n")
        .await
        .expect("ввод пишется");
    wait("привет, @alice").await;
    wait("токен обновлён по refresh").await;
    keys.write_all("post после продления\n".as_bytes())
        .await
        .expect("ввод пишется");
    wait("@alice [1] после продления").await;
    keys.write_all(b"quit\n").await.expect("ввод пишется");
    let _ = within("клиент выходит", session).await;
    reader.abort();
    stack.stop().await;
}
