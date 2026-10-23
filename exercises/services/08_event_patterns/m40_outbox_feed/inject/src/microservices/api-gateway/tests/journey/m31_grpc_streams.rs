use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api_gateway::{Config, Routes, Services, run_with};
use contracts::{Changed, ChatHistory, FeedPush, MessageView, PostId, PostView, Push};
use grpc_kit::{Access, LogSink};
use service_runtime::CancellationToken;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::task::JoinHandle;

use crate::fakes::{fake_auth, fake_chat, slow_users};
use crate::support::{Gateway, Peer, http, listener, ok, within};

struct Extra {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<api_gateway::Result<()>>,
}

impl Extra {
    async fn start(config: Config) -> Extra {
        let (http, addr) = listener().await;
        let stop = CancellationToken::new();
        let task = tokio::spawn(run_with(http, config, Routes::social(), stop.clone()));
        Extra { addr, stop, task }
    }

    async fn peer(&self, token: &str) -> Peer {
        Peer::connect(self.addr, &format!("/ws?token={token}")).await
    }

    async fn health(&self) -> (u16, String) {
        let answer = http(self.addr, "GET", "/health", None).await;
        (answer.status, answer.body)
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

fn post_of(feed: FeedPush) -> (PostId, bool) {
    match feed {
        FeedPush::FeedPost { post, live, .. } => (post.id, live),
        other => panic!("ждали пост ленты, а пришло {other:?}"),
    }
}

#[tokio::test]
async fn messages_travel_through_chat_service_to_every_device() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut phone) = stack.member("alice").await;
    let mut laptop = stack.device("alice").await;
    let (bob, mut other) = stack.member("bob").await;
    let sent = ok!(other, "chat.send", format!(r#"{{"peer": "{}", "text": "через chat-service"}}"#, alice.id) => MessageView);
    assert_eq!(
        (sent.from, sent.to),
        (bob.id, alice.id),
        "отправитель из сокета, а не из data"
    );
    for device in [&mut phone, &mut laptop] {
        assert_eq!(
            device.push().await,
            Push::Message {
                to: alice.id,
                from: bob.id
            },
            "сообщение приходит из потока Subscribe chat-service на каждое устройство alice"
        );
    }
    let history = ok!(phone, "chat.history", format!(r#"{{"peer": "{}"}}"#, bob.id) => ChatHistory);
    assert_eq!(
        history
            .messages
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        ["через chat-service"]
    );
    let ghost = other
        .fails("chat.send", r#"{"peer": "u_99", "text": "кто ты"}"#)
        .await;
    assert_eq!(
        ghost.code, "not_found",
        "chat-service спрашивает user-service: {ghost:?}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn one_subscription_per_user_opens_and_closes_with_devices() {
    let stack = Gateway::start(Config::default()).await;
    let stop = CancellationToken::new();
    let (chat, mut open) = fake_chat(&stop).await;
    let extra = Extra::start(Config {
        services: Services {
            chat: chat.to_string(),
            feed: String::from("127.0.0.1:9"),
            ..stack.services.clone()
        },
        ..Config::default()
    })
    .await;
    stack.register("alice").await;
    stack.register("bob").await;
    let first = extra.peer(&stack.login("alice").await.token).await;
    assert_eq!(
        *open.borrow(),
        1,
        "поток Subscribe открыт до ответа 101: сообщение, отправленное сразу после входа, не потеряется"
    );
    let second = extra.peer(&stack.login("alice").await.token).await;
    let bob = extra.peer(&stack.login("bob").await.token).await;
    assert_eq!(
        *open.borrow(),
        2,
        "один поток на пользователя, сколько бы у него ни было устройств: alice и bob"
    );
    drop(first);
    drop(bob);
    let wait = |open: &mut tokio::sync::watch::Receiver<usize>, n: usize| {
        let mut open = open.clone();
        async move {
            within(
                &format!("открытых подписок {n}"),
                open.wait_for(|now| *now == n),
            )
            .await
            .map(|_| ())
            .expect("счётчик подписок жив");
        }
    };
    wait(&mut open, 1).await;
    assert_eq!(
        *open.borrow(),
        1,
        "у alice ещё есть устройство: её поток живёт"
    );
    drop(second);
    wait(&mut open, 0).await;
    extra.stop().await;
    stop.cancel();
    stack.stop().await;
}

#[tokio::test]
async fn budget_cuts_slow_calls_and_travels_down() {
    let stop = CancellationToken::new();
    let (users, calls) = slow_users(&stop, Duration::from_secs(2)).await;
    let auth = fake_auth(&stop).await;
    let (posts_listener, posts) = listener().await;
    let posts_config = post_service::Config {
        users: users.to_string(),
        ..post_service::Config::default()
    };
    let post_task = tokio::spawn(post_service::run(
        posts_listener,
        posts_config,
        stop.clone(),
    ));
    let (dead, chat) = listener().await;
    drop(dead);
    let extra = Extra::start(Config {
        services: Services {
            auth: auth.to_string(),
            auth_http: String::from("127.0.0.1:9"),
            users: users.to_string(),
            posts: posts.to_string(),
            chat: chat.to_string(),
            feed: String::from("127.0.0.1:9"),
        },
        budget: Duration::from_millis(300),
        ..Config::default()
    })
    .await;
    let mut alice = extra.peer("alice-token").await;
    let started = Instant::now();
    let slow = alice.fails("posts.create", r#"{"text": "не успею"}"#).await;
    let spent = started.elapsed();
    assert!(
        spent < Duration::from_millis(1500),
        "WS-запрос получает ответ по бюджету 300 мс, а не ждёт медленный user-service 2 с: {spent:?}"
    );
    assert_eq!(slow.code, "unavailable", "{slow:?}");
    let seen = calls.take_seen();
    let profile = seen
        .iter()
        .find(|call| call.method == "GetProfile")
        .unwrap_or_else(|| panic!("post-service спросил автора: {seen:?}"));
    let budget = profile.budget.expect("до user-service доехал grpc-timeout");
    assert!(
        budget < Duration::from_millis(300),
        "post-service передаёт дальше остаток бюджета шлюза, а не свой новый срок: {budget:?}"
    );
    assert_eq!(
        profile.user.as_deref(),
        Some("1"),
        "перехватчик кладёт x-user-id в каждый вызов, и post-service пробрасывает его"
    );
    extra.stop().await;
    stop.cancel();
    let finished = within("остановка post-service", post_task)
        .await
        .expect("задача post-service не падает");
    assert!(finished.is_ok(), "{finished:?}");
}

#[tokio::test]
async fn health_says_which_service_is_missing() {
    let stack = Gateway::start(Config::default()).await;
    let stop = CancellationToken::new();
    let (users, _) = slow_users(&stop, Duration::ZERO).await;
    let extra = Extra::start(Config {
        services: Services {
            users: users.to_string(),
            ..stack.services.clone()
        },
        ..Config::default()
    })
    .await;
    let (status, body) = within("шлюз ждёт только user-service", async {
        loop {
            let (status, body) = extra.health().await;
            if !body.contains("auth-service") && !body.contains("post-service") {
                return (status, body);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        status == 503 && body.contains("user-service") && body.contains("unavailable"),
        "фейк без tonic-health не говорит SERVING, и /health шлюза называет именно его: {status} {body}"
    );
    let ready = http(stack.http, "GET", "/health", None).await;
    assert_eq!(ready.status, 200, "шлюз с настоящими сервисами готов");
    extra.stop().await;
    stop.cancel();
    stack.stop().await;
}

#[tokio::test]
async fn live_feed_streams_history_then_new_posts() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut stranger) = stack.member("carol").await;
    let (bob, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let first = ok!(writer, "posts.create", r#"{"text": "первый"}"# => PostView);
    let second = ok!(writer, "posts.create", r#"{"text": "второй"}"# => PostView);
    let _ = ok!(stranger, "posts.create", r#"{"text": "чужой"}"# => PostView);
    let started = ok!(reader, "posts.watch", r#"{"recent": 5}"# => Changed);
    assert!(started.changed);
    assert_eq!(
        [post_of(reader.feed().await), post_of(reader.feed().await)],
        [(second.id, false), (first.id, false)],
        "сначала история подписок, свежие первыми"
    );
    assert_eq!(
        reader.feed().await,
        FeedPush::FeedCaughtUp { to: bob.id },
        "потом отметка: дальше только новое"
    );
    let _ = ok!(stranger, "posts.create", r#"{"text": "чужой новый"}"# => PostView);
    let third = ok!(writer, "posts.create", r#"{"text": "третий"}"# => PostView);
    assert_eq!(
        post_of(reader.feed().await),
        (third.id, true),
        "новый пост подписки приходит потоком из post-service, чужой не приходит"
    );
    let stopped = ok!(reader, "posts.unwatch", "{}" => Changed);
    assert!(stopped.changed, "живая лента была и выключилась");
    let _ = ok!(writer, "posts.create", r#"{"text": "после"}"# => PostView);
    let _ = ok!(writer, "users.follow", format!(r#"{{"target": "{}"}}"#, bob.id) => Changed);
    let followed = within("пуш о подписке alice", async {
        loop {
            match reader.push().await {
                Push::NewPost { .. } => {}
                other => return other,
            }
        }
    })
    .await;
    assert_eq!(
        followed,
        Push::Followed {
            to: bob.id,
            by: alice.id
        },
        "пуши NewPost идут как раньше, а за ними подписка"
    );
    assert_eq!(
        reader.pending_feeds(),
        0,
        "после posts.unwatch лента молчит: подписка alice пришла, а поста «после» нет"
    );
    let again = ok!(reader, "posts.unwatch", "{}" => Changed);
    assert!(!again.changed, "выключать нечего");
    let bad = reader.fails("posts.watch", r#"{"recent": 0}"#).await;
    assert_eq!(bad.code, "bad_data", "{bad:?}");
    stack.stop().await;
}

#[tokio::test]
async fn access_log_lines_share_the_request_id() {
    let lines = Arc::new(Mutex::new(Vec::<Access>::new()));
    let sink = {
        let lines = Arc::clone(&lines);
        LogSink::new(move |access| {
            lines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(access.clone())
        })
    };
    let stack = Gateway::logged(Config::default(), sink).await;
    let (alice, mut peer) = stack.member("alice").await;
    lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = ok!(peer, "posts.create", r#"{"text": "в журнал"}"# => PostView);
    let seen = lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let create = seen
        .iter()
        .find(|line| line.path == "/social.v1.Posts/CreatePost")
        .unwrap_or_else(|| panic!("post-service записал CreatePost: {seen:?}"));
    let profile = seen
        .iter()
        .find(|line| line.path == "/social.v1.Users/GetProfile")
        .unwrap_or_else(|| panic!("user-service записал GetProfile: {seen:?}"));
    let me = alice.id.0.to_string();
    assert_eq!(
        (create.service, create.code, create.user.as_deref()),
        ("post-service", 0, Some(me.as_str())),
        "{create}"
    );
    assert_eq!(
        (profile.service, profile.request_id.as_str()),
        ("user-service", create.request_id.as_str()),
        "строки двух сервисов про один WS-запрос связаны одним x-request-id: {profile} и {create}"
    );
    assert_ne!(create.request_id, "-");
    stack.stop().await;
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
async fn course_client_watches_the_feed() {
    let stack = Gateway::start(Config::default()).await;
    let (_, mut writer) = stack.member("alice").await;
    stack.register("bob").await;
    let _ = ok!(writer, "posts.create", r#"{"text": "давнее"}"# => PostView);
    let http = stack.http.to_string();
    let (mut keys, stdin) = tokio::io::duplex(4096);
    let (stdout, screen) = tokio::io::duplex(1 << 16);
    let bob =
        tokio::spawn(async move { social_cli::run(&http, BufReader::new(stdin), stdout).await });
    let mut screen = BufReader::new(screen).lines();
    keys.write_all(b"login bob correct-horse\nfollow alice\nfeed --live 5\n")
        .await
        .expect("ввод bob пишется");
    let mut seen = Vec::new();
    for needle in ["живая лента включена", "давнее", "дальше только новые"]
    {
        if !seen.iter().any(|line: &String| line.contains(needle)) {
            seen.extend(read_until(&mut screen, needle).await);
        }
    }
    let fresh = ok!(writer, "posts.create", r#"{"text": "только что"}"# => PostView);
    let seen = read_until(&mut screen, "только что").await;
    let number = fresh.id.0;
    assert!(
        seen.iter()
            .any(|line| line.starts_with("* новое в ленте: @alice")
                && line.contains(&format!("[{number}] только что"))),
        "новый пост alice появляется у bob сам, без команды feed: {seen:?}"
    );
    keys.write_all(b"feed --stop\nquit\n").await.expect("ввод");
    read_until(&mut screen, "живая лента выключена").await;
    within("сеанс bob", bob)
        .await
        .expect("задача клиента не падает")
        .expect("сеанс bob проходит");
    stack.stop().await;
}
