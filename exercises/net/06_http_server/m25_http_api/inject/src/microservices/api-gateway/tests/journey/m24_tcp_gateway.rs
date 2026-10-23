use std::path::PathBuf;
use std::time::Duration;

use api_gateway::Config;
use contracts::{
    Changed, ChatHistory, FeedPage, MessageView, Outcome, PostId, PostView, Push, Response,
    UserView, decode,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::time::timeout;

use crate::support::{Gateway, Line, ok, within};

#[tokio::test]
async fn hello_introduces_the_connection() {
    let gateway = Gateway::start(Config::default()).await;
    let alice = gateway.register("alice").await;
    let bob = gateway.register("bob").await;
    let mut peer = gateway.peer().await;
    let before = peer.fails("posts.create", r#"{"text": "без имени"}"#).await;
    assert_eq!(
        before.code, "unauthorized",
        "пока соединение не представилось, действовать от чьего-то имени нельзя: {before:?}"
    );
    let login = gateway.login("Alice").await;
    let me = peer.hello(&login.token).await;
    assert_eq!(
        (me.id, me.username.as_str()),
        (alice.id, "alice"),
        "session.hello по токену отвечает тем, кто входил"
    );
    let post = ok!(peer, "posts.create", r#"{"text": "Привет из сети #tcp"}"# => PostView);
    assert_eq!(
        (post.author, post.text.as_str()),
        (alice.id, "Привет из сети #tcp"),
        "автор поста берётся из сессии"
    );
    let mut other = gateway.peer().await;
    other.hello(&gateway.login("bob").await.token).await;
    let followed =
        ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    assert!(followed.changed, "bob подписался на alice");
    let feed = ok!(other, "posts.feed", "{}" => FeedPage);
    assert_eq!(
        feed.posts
            .iter()
            .map(|post| post.id)
            .collect::<Vec<PostId>>(),
        vec![post.id],
        "в ленте bob пост alice"
    );
    let same = ok!(other, "users.follow", format!(r#"{{"user": "{}", "target": "{}"}}"#, bob.id, alice.id) => Changed);
    assert!(!same.changed, "свой user в data разрешён");
    gateway.stop().await;
}

#[tokio::test]
async fn session_rules_are_enforced() {
    let gateway = Gateway::start(Config::default()).await;
    let alice = gateway.register("alice").await;
    let bob = gateway.register("bob").await;
    let mut peer = gateway.peer().await;
    let stranger = peer
        .fails(
            "session.hello",
            r#"{"token": "0123456789abcdef0123456789abcdef"}"#,
        )
        .await;
    assert_eq!(stranger.code, "unauthorized", "чужой токен это отказ");
    let crooked = peer
        .fails("session.hello", r#"{"username": "alice"}"#)
        .await;
    assert_eq!(
        crooked.code, "bad_data",
        "ника в hello больше нет, только токен"
    );
    let login = gateway.login("alice").await;
    peer.hello(&login.token).await;
    let twice = peer
        .fails(
            "session.hello",
            &format!(r#"{{"token": "{}"}}"#, login.token),
        )
        .await;
    assert_eq!(twice.code, "conflict", "второй hello без bye это конфликт");
    let forged = peer
        .fails(
            "posts.create",
            &format!(r#"{{"user": "{}", "text": "я bob"}}"#, bob.id),
        )
        .await;
    assert_eq!(forged.code, "forbidden", "от чужого имени нельзя");
    let gone = ok!(peer, "session.bye", "{}" => UserView);
    assert_eq!(gone.id, alice.id, "bye отвечает тем, кто вышел");
    let again = peer.fails("session.bye", "{}").await;
    assert_eq!(again.code, "unauthorized", "bye без сессии это отказ");
    let found = ok!(peer, "users.find", r#"{"username": "bob"}"# => UserView);
    assert_eq!(found.id, bob.id, "искать можно и без сессии");
    let odd = peer.fails("session.dance", "{}").await;
    assert_eq!(odd.code, "unknown_method", "у сессии только hello и bye");
    gateway.stop().await;
}

#[tokio::test]
async fn pushes_reach_every_connection_of_the_user() {
    let gateway = Gateway::start(Config::default()).await;
    let (alice, mut phone) = gateway.member("alice").await;
    let login = gateway.login("alice").await;
    let mut laptop = gateway.peer().await;
    laptop.hello(&login.token).await;
    let (bob, mut other) = gateway.member("bob").await;
    let _ = ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let _ = ok!(other, "chat.send", format!(r#"{{"peer": "{}", "text": "привет"}}"#, alice.id) => MessageView);
    for device in [&mut phone, &mut laptop] {
        assert_eq!(
            device.push().await,
            Push::Followed {
                to: alice.id,
                by: bob.id
            },
            "подписка приходит push во все соединения alice"
        );
        assert_eq!(
            device.push().await,
            Push::Message {
                to: alice.id,
                from: bob.id
            },
            "личное сообщение приходит push во все соединения alice"
        );
    }
    let history = ok!(phone, "chat.history", format!(r#"{{"peer": "{}"}}"#, bob.id) => ChatHistory);
    assert_eq!(history.messages.len(), 1, "сообщение лежит в истории");
    let post = ok!(laptop, "posts.create", r#"{"text": "Новый пост"}"# => PostView);
    assert_eq!(
        other.push().await,
        Push::NewPost {
            to: bob.id,
            author: alice.id,
            post: post.id
        },
        "подписчик получает push о новом посте"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn garbage_and_long_lines() {
    let gateway = Gateway::start(Config {
        max_line: 1024,
        ..Config::default()
    })
    .await;
    let mut peer = gateway.peer().await;
    peer.raw("это не JSON").await;
    let Some(Line::Reply(raw)) = peer.line().await else {
        panic!("на мусор шлюз отвечает строкой ошибки");
    };
    let response: Response<Changed> =
        decode(&raw).unwrap_or_else(|e| panic!("ответ на мусор разбирается ({e}): {raw}"));
    assert!(
        matches!(&response.outcome, Outcome::Error(body) if body.code == "syntax")
            && response.id.is_none(),
        "мусор это ошибка syntax без id: {raw}"
    );
    let _ = ok!(peer, "users.register", r#"{"username": "alice"}"# => UserView);
    peer.raw(&"x".repeat(4096)).await;
    let Some(Line::Reply(raw)) = peer.line().await else {
        panic!("на слишком длинную строку шлюз отвечает ошибкой");
    };
    assert!(
        raw.contains("too_long"),
        "строка длиннее max_line это too_long: {raw}"
    );
    assert!(
        peer.closed().await,
        "после слишком длинной строки соединение закрыто"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn idle_connection_is_closed() {
    let gateway = Gateway::start(Config {
        idle_timeout: Duration::from_millis(300),
        ..Config::default()
    })
    .await;
    let mut busy = gateway.peer().await;
    let mut idle = gateway.peer().await;
    let _ = ok!(busy, "users.register", r#"{"username": "alice"}"# => UserView);
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = ok!(busy, "users.find", r#"{"username": "alice"}"# => UserView);
    }
    assert!(
        idle.closed().await,
        "соединение без единой строки дольше idle_timeout закрыто"
    );
    let _ = ok!(busy, "users.find", r#"{"username": "alice"}"# => UserView);
    gateway.stop().await;
}

#[tokio::test]
async fn connection_limit_holds_extra_clients() {
    let gateway = Gateway::start(Config {
        max_connections: 2,
        ..Config::default()
    })
    .await;
    let mut first = gateway.peer().await;
    let _ = ok!(first, "users.register", r#"{"username": "alice"}"# => UserView);
    let mut second = gateway.peer().await;
    let _ = ok!(second, "users.find", r#"{"username": "alice"}"# => UserView);
    let mut third = gateway.peer().await;
    let id = third.send("users.find", r#"{"username": "alice"}"#).await;
    let early = timeout(Duration::from_millis(400), third.lines.next_line()).await;
    assert!(
        early.is_err(),
        "третье соединение сверх max_connections = 2 ждёт своей очереди"
    );
    drop(first);
    let raw = within("ответ третьему", third.reply(id)).await;
    assert!(
        raw.contains("alice"),
        "место освободилось, третье соединение обслужено: {raw}"
    );
    gateway.stop().await;
}

fn journal(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m24");
    let _ = std::fs::remove_dir_all(dir.join(name));
    dir.join(name).join("journal.ndjson")
}

#[tokio::test]
async fn shutdown_closes_connections_and_keeps_the_journal() {
    let config = Config {
        journal: Some(journal("shutdown")),
        ..Config::default()
    };
    let gateway = Gateway::start(config.clone()).await;
    let (_, mut peer) = gateway.member("alice").await;
    let _ = ok!(peer, "posts.create", r#"{"text": "До перезапуска"}"# => PostView);
    gateway.stop().await;
    assert!(
        peer.closed().await,
        "при остановке шлюз закрывает открытые соединения"
    );

    let gateway = Gateway::start(config).await;
    let login = gateway.login("alice").await;
    let mut peer = gateway.peer().await;
    let me = peer.hello(&login.token).await;
    let feed = ok!(peer, "posts.feed", "{}" => FeedPage);
    assert_eq!(
        (me.username.as_str(), feed.posts.len()),
        ("alice", 1),
        "шлюз поднимает ядро из журнала: alice и её пост пережили перезапуск"
    );
    gateway.stop().await;
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
async fn course_client_talks_to_the_gateway() {
    let gateway = Gateway::start(Config::default()).await;
    let (tcp, http) = (gateway.tcp.to_string(), gateway.http.to_string());
    let (mut keys, stdin) = tokio::io::duplex(4096);
    let (stdout, screen) = tokio::io::duplex(1 << 16);
    let alice = tokio::spawn({
        let (tcp, http) = (tcp.clone(), http.clone());
        async move { social_cli::run(&tcp, &http, BufReader::new(stdin), stdout).await }
    });
    let mut screen = BufReader::new(screen).lines();
    keys.write_all("register alice\nlogin alice\npost Привет\n".as_bytes())
        .await
        .expect("ввод alice пишется");
    read_until(&mut screen, "привет, @alice").await;
    read_until(&mut screen, "@alice [1] Привет (лайков: 0)").await;
    let mut bob_screen = Vec::new();
    within(
        "сеанс bob",
        social_cli::run(
            &tcp,
            &http,
            "register bob\nlogin bob\nfollow alice\nfeed\nshow 1\nprofile alice\nmsg alice как дела?\nquit\n".as_bytes(),
            &mut bob_screen,
        ),
    )
    .await
    .expect("сеанс bob проходит");
    let bob_screen = String::from_utf8(bob_screen).expect("клиент пишет UTF-8");
    assert!(
        bob_screen.matches("@alice [1] Привет (лайков: 0)").count() == 2
            && bob_screen.contains("профиль @alice (u_1)"),
        "bob видит пост alice в ленте и по ссылке, профиль alice по ссылке:\n{bob_screen}"
    );
    let seen = read_until(&mut screen, ": как дела?").await;
    assert!(
        seen.iter()
            .any(|line| line.starts_with("* на тебя подписался ")),
        "alice видит push о подписке: {seen:?}"
    );
    keys.write_all(b"quit\n").await.expect("ввод alice пишется");
    within("сеанс alice", alice)
        .await
        .expect("задача клиента не падает")
        .expect("сеанс alice проходит");
    gateway.stop().await;
}
