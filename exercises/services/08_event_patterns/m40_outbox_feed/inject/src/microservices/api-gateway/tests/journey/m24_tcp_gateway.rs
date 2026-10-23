use std::time::Duration;

use api_gateway::Config;
use contracts::{
    Changed, ChatHistory, MessageView, Outcome, PostId, PostView, Push, Response, UserView, decode,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};

use crate::support::{Gateway, Journals, Line, ok, within};

#[tokio::test]
async fn token_introduces_the_connection() {
    let gateway = Gateway::start(Config::default()).await;
    let alice = gateway.register("alice").await;
    let bob = gateway.register("bob").await;
    let mut guest = crate::support::Peer::connect(gateway.http, "/ws").await;
    let denied = guest.fails("posts.create", r#"{"text": "аноним"}"#).await;
    assert_eq!(
        denied.code, "unauthorized",
        "без токена сокет гостевой (с m37): писать от имени пользователя нельзя"
    );
    let mut peer = gateway.device("Alice").await;
    let post = ok!(peer, "posts.create", r#"{"text": "Привет из сети #tcp"}"# => PostView);
    assert_eq!(
        (post.author, post.text.as_str()),
        (alice.id, "Привет из сети #tcp"),
        "автор поста берётся из токена, с которым открыт сокет"
    );
    let mut other = gateway.device("bob").await;
    let followed =
        ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    assert!(followed.changed, "bob подписался на alice");
    let feed = gateway
        .feed_until(
            &mut other,
            "пост alice у подписчика",
            |page| !page.posts.is_empty(),
        )
        .await;
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
async fn acting_rules_are_enforced() {
    let gateway = Gateway::start(Config::default()).await;
    let (_, mut peer) = gateway.member("alice").await;
    let bob = gateway.register("bob").await;
    let forged = peer
        .fails(
            "posts.create",
            &format!(r#"{{"user": "{}", "text": "я bob"}}"#, bob.id),
        )
        .await;
    assert_eq!(forged.code, "forbidden", "от чужого имени нельзя");
    let found = ok!(peer, "users.find", r#"{"username": "bob"}"# => UserView);
    assert_eq!(found.id, bob.id, "поиск открыт любому сокету");
    let register = peer
        .fails("users.register", r#"{"username": "carol"}"#)
        .await;
    assert_eq!(
        register.code, "unknown_method",
        "регистрация живёт только в HTTP: POST /auth/register"
    );
    let hello = peer
        .fails(
            "session.hello",
            r#"{"token": "0123456789abcdef0123456789abcdef"}"#,
        )
        .await;
    assert_eq!(
        hello.code, "unknown_method",
        "сокет представлен с рукопожатия: методов session.* больше нет"
    );
    let odd = peer.fails("posts.dance", "{}").await;
    assert_eq!(odd.code, "unknown_method", "метода нет в таблице");
    gateway.stop().await;
}

#[tokio::test]
async fn pushes_reach_every_connection_of_the_user() {
    let gateway = Gateway::start(Config::default()).await;
    let (alice, mut phone) = gateway.member("alice").await;
    let mut laptop = gateway.device("alice").await;
    let (bob, mut other) = gateway.member("bob").await;
    let _ = ok!(other, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let _ = ok!(other, "chat.send", format!(r#"{{"peer": "{}", "text": "привет"}}"#, alice.id) => MessageView);
    for device in [&mut phone, &mut laptop] {
        let pair = [device.push().await, device.push().await];
        let followed = Push::Followed {
            to: alice.id,
            by: bob.id,
        };
        let message = Push::Message {
            to: alice.id,
            from: bob.id,
        };
        assert!(
            pair.contains(&followed),
            "подписка приходит push во все соединения alice: {pair:?}"
        );
        assert!(
            pair.contains(&message),
            "личное сообщение приходит push во все соединения alice: {pair:?}"
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
    let (_, mut peer) = gateway.member("alice").await;
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
    let _ = ok!(peer, "users.find", r#"{"username": "alice"}"# => UserView);
    peer.raw(&"x".repeat(4096)).await;
    assert!(
        peer.closed().await,
        "сообщение длиннее max_line закрывает WebSocket"
    );
    assert_eq!(
        peer.close_code(),
        Some(1009),
        "слишком большое сообщение это код закрытия 1009"
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
    let (_, mut busy) = gateway.member("alice").await;
    let (_, mut idle) = gateway.member("bob").await;
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = ok!(busy, "users.find", r#"{"username": "alice"}"# => UserView);
    }
    assert!(
        idle.closed().await,
        "соединение без единого сообщения дольше idle_timeout закрыто"
    );
    assert_eq!(
        idle.close_code(),
        Some(1001),
        "простой это код закрытия 1001"
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
    let (_, mut first) = gateway.member("alice").await;
    let _ = ok!(first, "users.find", r#"{"username": "alice"}"# => UserView);
    let mut second = gateway.device("alice").await;
    let _ = ok!(second, "users.find", r#"{"username": "alice"}"# => UserView);
    let http = gateway.http;
    let path = format!("/ws?token={}", gateway.login("alice").await.token);
    let third = tokio::spawn(async move {
        let mut third = crate::support::Peer::connect(http, &path).await;
        ok!(third, "users.find", r#"{"username": "alice"}"# => UserView)
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        !third.is_finished(),
        "третье соединение сверх max_connections = 2 ждёт своей очереди, а не обслужено сразу"
    );
    drop(first);
    let found = within("ответ третьему", third)
        .await
        .expect("задача третьего клиента не падает");
    assert_eq!(
        found.username, "alice",
        "место освободилось, третье соединение обслужено"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn shutdown_closes_connections_and_keeps_the_journal() {
    let journals = Journals::fresh("m24", "shutdown");
    let gateway = Gateway::with_journals(Config::default(), &journals).await;
    let (_, mut peer) = gateway.member("alice").await;
    let _ = ok!(peer, "posts.create", r#"{"text": "До перезапуска"}"# => PostView);
    gateway.stop().await;
    assert!(
        peer.closed().await,
        "при остановке шлюз закрывает открытые соединения"
    );
    assert_eq!(
        peer.close_code(),
        Some(1001),
        "остановка шлюза это код закрытия 1001: going away"
    );

    let gateway = Gateway::with_journals(Config::default(), &journals).await;
    let mut peer = gateway.device("alice").await;
    let me = ok!(peer, "users.find", r#"{"username": "alice"}"# => UserView);
    let feed = gateway
        .feed_until(
            &mut peer,
            "пост alice после перезапуска",
            |page| !page.posts.is_empty(),
        )
        .await;
    assert_eq!(
        (me.username.as_str(), feed.posts.len()),
        ("alice", 1),
        "сервисы поднимаются из своих журналов: alice и её пост пережили перезапуск"
    );
    gateway.stop().await;
    journals.forget().await;
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
    let http = gateway.http.to_string();
    let (mut keys, stdin) = tokio::io::duplex(4096);
    let (stdout, screen) = tokio::io::duplex(1 << 16);
    let alice = tokio::spawn({
        let http = http.clone();
        async move { social_cli::run(&http, BufReader::new(stdin), stdout).await }
    });
    let mut screen = BufReader::new(screen).lines();
    keys.write_all(
        "register alice correct-horse\nlogin alice correct-horse\npost Привет\n".as_bytes(),
    )
    .await
    .expect("ввод alice пишется");
    read_until(&mut screen, "привет, @alice").await;
    read_until(&mut screen, "@alice [1] Привет (лайков: 0)").await;
    let (_, mut follower) = gateway.member("bob").await;
    let _ = ok!(follower, "users.follow", r#"{"target": "u_1"}"# => Changed);
    gateway
        .feed_until(
            &mut follower,
            "подписка bob и пост alice дошли до feed-service",
            |page| !page.posts.is_empty(),
        )
        .await;
    let mut bob_screen = Vec::new();
    within(
        "сеанс bob",
        social_cli::run(
            &http,
            "login bob correct-horse\nfeed\nshow 1\nprofile alice\nmsg alice как дела?\nquit\n"
                .as_bytes(),
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
