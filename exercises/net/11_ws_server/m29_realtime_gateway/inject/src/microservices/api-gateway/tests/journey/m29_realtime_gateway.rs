use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use api_gateway::{Config, Ctx, Out, Outbox, Registry, Routes, TOO_SLOW};
use contracts::{Changed, ChatHistory, Envelope, METHOD_NAMES, Push, Reply, UserId};
use social_core::{Notification, Notifier};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpStream;

use crate::support::{Gateway, id_of, ok, within};

async fn handshake(addr: SocketAddr, path: &str, extra: &[(&str, &str)]) -> String {
    let mut stream = within("подключение", TcpStream::connect(addr))
        .await
        .expect("HTTP-порт открыт");
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
    );
    for (name, value) in extra {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("запрос уходит");
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let n = within("ответ на рукопожатие", stream.read(&mut byte))
            .await
            .expect("ответ читается");
        if n == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

fn status(head: &str) -> u16 {
    head.split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_default()
}

fn changed() -> Reply {
    Reply::Changed(Changed { changed: true })
}

fn with_naps() -> Routes {
    let nap = |ms: u64| {
        move |_: Ctx, _: Envelope| async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            Ok(changed())
        }
    };
    Routes::social()
        .route("debug.slow", nap(600))
        .route("debug.middle", nap(300))
        .route("debug.fast", nap(100))
        .route("debug.nap", nap(200))
        .route("debug.boom", |_: Ctx, envelope: Envelope| async move {
            if envelope.id > 0 {
                panic!("обработчик падает нарочно");
            }
            Ok(changed())
        })
}

fn queued(rx: &mut tokio::sync::mpsc::Receiver<Out>) -> Vec<Out> {
    let mut all = Vec::new();
    while let Ok(item) = rx.try_recv() {
        all.push(item);
    }
    all
}

#[test]
fn registry_forgets_connections_by_itself() {
    let registry = Arc::new(Registry::default());
    let (alice, bob) = (UserId(1), UserId(2));
    let (phone, mut phone_rx) = registry.outbox(8);
    let (laptop, mut laptop_rx) = registry.outbox(8);
    let (other, _other_rx) = registry.outbox(8);
    let on_phone = registry.register(alice, phone);
    let on_laptop = registry.register(alice, laptop);
    let on_other = registry.register(bob, other);
    assert_ne!(
        on_phone.conn(),
        on_laptop.conn(),
        "у каждого соединения свой номер"
    );
    assert_eq!(
        (
            registry.devices(alice),
            registry.users(),
            registry.connections()
        ),
        (2, 2, 3),
        "alice с двух устройств, bob с одного"
    );
    registry.notify(alice, Notification::Followed { by: bob });
    let event = String::from(r#"{"event":"followed","data":{"to":"u_1","by":"u_2"}}"#);
    for rx in [&mut phone_rx, &mut laptop_rx] {
        assert_eq!(
            queued(rx),
            vec![Out::Text(event.clone())],
            "push уходит на каждое устройство пользователя в виде {{event, data}}"
        );
    }
    drop(on_phone);
    assert_eq!(
        registry.devices(alice),
        1,
        "Drop гарда убирает соединение из реестра"
    );
    assert_eq!(
        registry.deliver(
            alice,
            &Push::Message {
                to: alice,
                from: bob
            }
        ),
        1,
        "push идёт только на оставшееся устройство"
    );
    drop(on_laptop);
    assert_eq!(
        (registry.devices(alice), registry.users()),
        (0, 1),
        "ушло последнее устройство: пользователя в реестре не осталось"
    );
    let unwinding = std::thread::spawn(move || {
        let _guard = on_other;
        panic!("соединение падает с паникой");
    });
    assert!(unwinding.join().is_err(), "поток упал, как и задумано");
    assert_eq!(
        registry.connections(),
        0,
        "гард убирает запись и при панике"
    );
}

#[test]
fn full_outbox_kicks_the_client_with_1008() {
    let registry = Registry::default();
    let (outbox, mut rx) = registry.outbox(2);
    assert!(outbox.send(String::from("a")) && outbox.send(String::from("b")));
    assert!(
        !outbox.send(String::from("c")),
        "очередь на 2 сообщения: третье уже не влезает"
    );
    assert!(outbox.is_kicked(), "переполнение отключает клиента");
    assert!(
        !outbox.send(String::from("d")),
        "отключённому больше не пишут"
    );
    outbox.kick();
    assert_eq!(
        queued(&mut rx),
        vec![
            Out::Text(String::from("a")),
            Out::Text(String::from("b")),
            Out::Close(TOO_SLOW),
        ],
        "место под Close зарезервировано: писатель вынет ровно один Close(1008)"
    );
    assert_eq!(
        (TOO_SLOW, registry.kicked()),
        (1008, 1),
        "отключение считается один раз"
    );
    let (lone, _rx) = Outbox::channel(1);
    assert!(lone.send(String::from("x")) && !lone.send(String::from("y")));
}

#[test]
fn routes_cover_the_contract() {
    let routes = Routes::social();
    let mut expected: Vec<&str> = METHOD_NAMES
        .into_iter()
        .filter(|method| *method != "users.register")
        .collect();
    expected.sort_unstable();
    assert_eq!(
        routes.names(),
        expected,
        "в таблице все методы контракта, кроме регистрации: она только по HTTP"
    );
    for name in routes.names() {
        assert_eq!(
            routes.is_authed(name),
            Some(name != "users.find"),
            "{name}: от имени пользователя действуют только маршруты route_authed"
        );
    }
    assert_eq!(routes.is_authed("users.register"), None);
    let twice = std::panic::catch_unwind(|| {
        Routes::new()
            .route("debug.a", |_: Ctx, _: Envelope| async { Ok(changed()) })
            .route("debug.a", |_: Ctx, _: Envelope| async { Ok(changed()) })
    });
    assert!(
        twice.is_err(),
        "метод, записанный дважды, это ошибка при старте, а не тихая подмена"
    );
}

#[tokio::test]
async fn token_is_checked_before_the_upgrade() {
    let gateway = Gateway::start(Config::default()).await;
    gateway.register("alice").await;
    let token = gateway.login("alice").await.token;
    let none = handshake(gateway.http, "/ws", &[]).await;
    assert_eq!(status(&none), 401, "без токена это 401 без 101: {none}");
    let stranger = handshake(
        gateway.http,
        "/ws?token=0123456789abcdef0123456789abcdef",
        &[],
    )
    .await;
    assert_eq!(status(&stranger), 401, "чужой токен это 401: {stranger}");
    let bearer = format!("Bearer {token}");
    let header = handshake(gateway.http, "/ws", &[("Authorization", &bearer)]).await;
    assert_eq!(
        status(&header),
        101,
        "мобильный клиент кладёт токен в Authorization: {header}"
    );
    let cookie = format!("session={token}");
    let browser = handshake(gateway.http, "/ws", &[("Cookie", &cookie)]).await;
    assert_eq!(
        status(&browser),
        101,
        "браузер со своей cookie сессии открывает сокет без токена в адресе: {browser}"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn pushes_are_events_mixed_with_replies() {
    let gateway = Gateway::start(Config::default()).await;
    let (alice, mut phone) = gateway.member("alice").await;
    let (bob, mut other) = gateway.member("bob").await;
    let _ = ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let id = phone.send("users.find", r#"{"username": "bob"}"#).await;
    let mut event = None;
    let mut reply = None;
    while event.is_none() || reply.is_none() {
        let raw = phone.text().await.expect("шлюз шлёт сообщения");
        if raw.starts_with(r#"{"event":"#) {
            event = Some(raw);
        } else {
            reply = Some(raw);
        }
    }
    assert_eq!(
        event.as_deref(),
        Some(
            format!(
                r#"{{"event":"followed","data":{{"to":"{}","by":"{}"}}}}"#,
                alice.id, bob.id
            )
            .as_str()
        ),
        "событие это {{event, data}} без id"
    );
    let reply = reply.unwrap_or_default();
    assert!(
        id_of(&reply) == Some(id) && !reply.contains(r#""event""#),
        "ответ несёт id запроса и не похож на событие: {reply}"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn requests_of_one_socket_run_in_parallel() {
    let gateway = Gateway::start_with(Config::default(), with_naps()).await;
    let (_, mut alice) = gateway.member("alice").await;
    let began = Instant::now();
    let slow = alice.send("debug.slow", "{}").await;
    let fast = alice.send("debug.fast", "{}").await;
    let middle = alice.send("debug.middle", "{}").await;
    let mut order = Vec::new();
    for _ in 0..3 {
        let raw = alice.any_reply().await;
        assert!(raw.contains(r#""ok""#), "запрос выполнен: {raw}");
        order.push(id_of(&raw).unwrap_or_default());
    }
    assert_eq!(
        order,
        vec![fast, middle, slow],
        "ответы уходят по готовности, каждый со своим id"
    );
    assert!(
        began.elapsed() < Duration::from_millis(950),
        "три запроса заняли {:?}: по очереди это 1 с, параллельно около 0,6 с",
        began.elapsed()
    );
    let boom = alice.fails("debug.boom", "{}").await;
    assert_eq!(
        boom.code, "internal",
        "паника обработчика это ответ internal с тем же id"
    );
    let found = ok!(alice, "users.find", r#"{"username": "alice"}"# => contracts::UserView);
    assert_eq!(found.username, "alice", "после паники сокет живёт");
    gateway.stop().await;
}

#[tokio::test]
async fn in_flight_limit_is_per_socket() {
    let config = Config {
        max_in_flight: 2,
        ..Config::default()
    };
    let gateway = Gateway::start_with(config, with_naps()).await;
    let (_, mut alice) = gateway.member("alice").await;
    let (_, mut bob) = gateway.member("bob").await;
    let began = Instant::now();
    for _ in 0..6 {
        alice.send("debug.nap", "{}").await;
    }
    for _ in 0..2 {
        bob.send("debug.nap", "{}").await;
    }
    for _ in 0..2 {
        let _ = bob.any_reply().await;
    }
    let other = began.elapsed();
    for _ in 0..6 {
        let raw = alice.any_reply().await;
        assert!(raw.contains(r#""ok""#), "запрос выполнен: {raw}");
    }
    let took = began.elapsed();
    assert!(
        took >= Duration::from_millis(560),
        "шесть запросов по 200 мс при лимите 2 это три волны, не меньше 0,6 с, а заняло {took:?}"
    );
    assert!(
        took < Duration::from_millis(1150),
        "лимит 2 даёт параллельность 2 (около 0,6 с), а заняло {took:?}"
    );
    assert!(
        other < Duration::from_millis(550),
        "лимит у каждого сокета свой: bob не ждёт очереди alice, а ждал {other:?}"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn slow_device_is_cut_off_and_nobody_else_waits() {
    let gateway = Gateway::start(Config {
        outbox: 4,
        ..Config::default()
    })
    .await;
    let (alice, mut laptop) = gateway.member("alice").await;
    let mut phone = gateway.device("alice").await;
    let (bob, mut sender) = gateway.member("bob").await;
    let text = "я".repeat(500);
    let message = format!(r#"{{"peer": "{}", "text": "{text}"}}"#, alice.id);
    for _ in 0..100 {
        let _ = ok!(sender, "chat.send", &message => contracts::MessageView);
    }
    for _ in 0..100 {
        assert_eq!(
            laptop.push().await,
            Push::Message {
                to: alice.id,
                from: bob.id
            }
        );
    }
    let history = ok!(laptop, "chat.history", format!(r#"{{"peer": "{}", "limit": 100}}"#, bob.id) => ChatHistory);
    assert_eq!(
        history.messages.len(),
        100,
        "переписка тяжёлая: 100 сообщений"
    );
    let heavy = format!(r#"{{"peer": "{}", "limit": 100}}"#, bob.id);
    for _ in 0..200 {
        phone.send("chat.history", &heavy).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let began = Instant::now();
    for _ in 0..20 {
        let _ = ok!(sender, "chat.send", format!(r#"{{"peer": "{}", "text": "ещё"}}"#, alice.id) => contracts::MessageView);
        assert_eq!(
            laptop.push().await,
            Push::Message {
                to: alice.id,
                from: bob.id
            },
            "ноутбук alice получает всё, пока телефон не читает"
        );
    }
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "медленный телефон не тормозит ни отправителя, ни второе устройство: {:?}",
        began.elapsed()
    );
    let seen = phone
        .closed_slowly(Duration::from_millis(20))
        .await
        .expect("медленный телефон в итоге отключён");
    assert!(
        seen < 200,
        "телефон отключён, не дочитав: получил {seen} сообщений"
    );
    assert_eq!(
        phone.close_code(),
        Some(1008),
        "клиент, который не успевает читать, отключается с кодом 1008"
    );
    let _ = ok!(laptop, "users.find", r#"{"username": "bob"}"# => contracts::UserView);
    gateway.stop().await;
}

#[tokio::test]
async fn heartbeat_keeps_live_sockets_and_drops_silent_ones() {
    let gateway = Gateway::start(Config {
        ping_every: Duration::from_millis(100),
        idle_timeout: Duration::from_millis(400),
        ..Config::default()
    })
    .await;
    let (_, mut alive) = gateway.member("alice").await;
    let (_, mut silent) = gateway.member("bob").await;
    let keeper = tokio::spawn(async move {
        let closed = alive.closed().await;
        (closed, alive.close_code())
    });
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        silent.closed().await,
        "сокет, который не отвечает на ping дольше idle_timeout, закрыт"
    );
    assert_eq!(
        silent.close_code(),
        Some(1001),
        "молчуну отвечают кодом 1001"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !keeper.is_finished(),
        "клиент, который отвечает на ping, живёт дольше idle_timeout"
    );
    gateway.stop().await;
    let (closed, code) = within("клиент с ping", keeper)
        .await
        .expect("задача читателя не падает");
    assert_eq!(
        (closed, code),
        (true, Some(1001)),
        "живой клиент закрыт только остановкой шлюза"
    );
}

async fn read_until(
    lines: &mut Lines<BufReader<tokio::io::DuplexStream>>,
    needle: &str,
) -> Vec<String> {
    let mut seen = Vec::new();
    loop {
        let line = within(&format!("строка клиента с «{needle}»"), lines.next_line())
            .await
            .expect("вывод клиента читается")
            .unwrap_or_else(|| panic!("клиент замолчал, так и не написав «{needle}»: {seen:?}"));
        let found = line.contains(needle);
        seen.push(line);
        if found {
            return seen;
        }
    }
}

#[tokio::test]
async fn two_windows_of_one_user_both_hear_everything() {
    let gateway = Gateway::start(Config::default()).await;
    let http = gateway.http.to_string();
    gateway.register("alice").await;
    let mut windows = Vec::new();
    for _ in 0..2 {
        let (mut keys, stdin) = tokio::io::duplex(4096);
        let (stdout, screen) = tokio::io::duplex(1 << 16);
        let task = tokio::spawn({
            let http = http.clone();
            async move { social_cli::run(&http, BufReader::new(stdin), stdout).await }
        });
        let mut screen = BufReader::new(screen).lines();
        keys.write_all(b"login alice\n")
            .await
            .expect("ввод alice пишется");
        read_until(&mut screen, "привет, @alice").await;
        windows.push((keys, screen, task));
    }
    let mut bob_screen = Vec::new();
    within(
        "сеанс bob",
        social_cli::run(
            &http,
            "register bob\nlogin bob\nfollow alice\nmsg alice два окна\nquit\n".as_bytes(),
            &mut bob_screen,
        ),
    )
    .await
    .expect("сеанс bob проходит");
    for (mut keys, mut screen, task) in windows {
        let seen = read_until(&mut screen, ": два окна").await;
        assert!(
            seen.iter()
                .any(|line| line.starts_with("* на тебя подписался ")),
            "оба окна alice видят подписку и сообщение: {seen:?}"
        );
        keys.write_all(b"quit\n").await.expect("ввод alice пишется");
        within("сеанс alice", task)
            .await
            .expect("задача клиента не падает")
            .expect("сеанс alice проходит");
    }
    gateway.stop().await;
}
