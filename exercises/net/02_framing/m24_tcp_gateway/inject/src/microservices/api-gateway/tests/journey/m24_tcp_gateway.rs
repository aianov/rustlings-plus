use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use api_gateway::{Config, run};
use contracts::{
    Changed, ChatHistory, ErrorBody, FeedPage, MessageView, Outcome, PostId, PostView, Push,
    Response, UserId, UserView, decode,
};
use service_runtime::CancellationToken;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

fn id_of(raw: &str) -> Option<u64> {
    let rest = raw.strip_prefix(r#"{"id":"#)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

enum Line {
    Push(Push),
    Reply(String),
}

macro_rules! ok {
    ($peer:expr, $method:expr, $data:expr => $ty:ty) => {{
        let method = $method;
        let data = $data;
        let raw = $peer.call(method, &data).await;
        match decode::<Response<$ty>>(&raw) {
            Ok(Response {
                outcome: Outcome::Ok(value),
                ..
            }) => value,
            Ok(Response {
                outcome: Outcome::Error(body),
                ..
            }) => panic!("{method} {data}: ждали успех, а пришло {body:?}"),
            Err(error) => panic!("ответ на {method} не разобрался ({error}): {raw}"),
        }
    }};
}

struct Gateway {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<api_gateway::Result<()>>,
}

impl Gateway {
    async fn start(config: Config) -> Gateway {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("порт 0 на 127.0.0.1 свободен всегда");
        let addr = listener.local_addr().expect("у слушателя есть адрес");
        let stop = CancellationToken::new();
        let task = tokio::spawn(run(listener, config, stop.clone()));
        Gateway { addr, stop, task }
    }

    async fn stop(self) {
        self.stop.cancel();
        let finished = within("остановка шлюза", self.task)
            .await
            .expect("задача шлюза не падает");
        if let Err(error) = finished {
            panic!("run завершился ошибкой: {error:?}");
        }
    }
}

struct Peer {
    lines: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
    next: u64,
    pushes: VecDeque<Push>,
}

impl Peer {
    async fn connect(addr: SocketAddr) -> Peer {
        let stream = within("подключение", TcpStream::connect(addr))
            .await
            .expect("шлюз принимает соединения");
        let (read, writer) = stream.into_split();
        Peer {
            lines: BufReader::new(read).lines(),
            writer,
            next: 0,
            pushes: VecDeque::new(),
        }
    }

    async fn raw(&mut self, line: &str) {
        self.writer
            .write_all(format!("{line}\n").as_bytes())
            .await
            .expect("строка уходит в шлюз");
    }

    async fn line(&mut self) -> Option<Line> {
        let raw = within("строка от шлюза", self.lines.next_line())
            .await
            .ok()
            .flatten()?;
        match decode::<Push>(&raw) {
            Ok(push) => Some(Line::Push(push)),
            Err(_) => Some(Line::Reply(raw)),
        }
    }

    async fn send(&mut self, method: &str, data: &str) -> u64 {
        self.next += 1;
        let service = method.split('.').next().unwrap_or(method);
        let line = format!(
            r#"{{"id": {}, "service": "{service}", "method": "{method}", "data": {data}}}"#,
            self.next
        );
        self.raw(&line).await;
        self.next
    }

    async fn reply(&mut self, id: u64) -> String {
        loop {
            match self.line().await {
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Reply(raw)) => {
                    if id_of(&raw) == Some(id) {
                        return raw;
                    }
                    panic!("ждали ответ на запрос {id}, а пришло {raw}");
                }
                None => panic!("шлюз закрыл соединение, не ответив на запрос {id}"),
            }
        }
    }

    async fn call(&mut self, method: &str, data: &str) -> String {
        let id = self.send(method, data).await;
        self.reply(id).await
    }

    async fn fails(&mut self, method: &str, data: &str) -> ErrorBody {
        let raw = self.call(method, data).await;
        match decode::<Response<Changed>>(&raw) {
            Ok(Response {
                outcome: Outcome::Error(body),
                ..
            }) => body,
            _ => panic!("{method} {data}: ждали ошибку, а пришло {raw}"),
        }
    }

    async fn hello(&mut self, name: &str) -> UserView {
        ok!(self, "session.hello", format!(r#"{{"username": "{name}"}}"#) => UserView)
    }

    async fn push(&mut self) -> Push {
        if let Some(push) = self.pushes.pop_front() {
            return push;
        }
        match self.line().await {
            Some(Line::Push(push)) => push,
            Some(Line::Reply(raw)) => panic!("ждали push, а пришёл ответ {raw}"),
            None => panic!("шлюз закрыл соединение, а push так и не пришёл"),
        }
    }

    async fn closed(&mut self) -> bool {
        matches!(
            timeout(DEADLINE, self.lines.next_line()).await,
            Ok(Ok(None) | Err(_))
        )
    }
}

async fn register(peer: &mut Peer, name: &str) -> UserId {
    let user = ok!(peer, "users.register", format!(r#"{{"username": "{name}"}}"#) => UserView);
    user.id
}

#[tokio::test]
async fn hello_introduces_the_connection() {
    let gateway = Gateway::start(Config::default()).await;
    let mut alice = Peer::connect(gateway.addr).await;
    let alice_id = register(&mut alice, "alice").await;
    let bob_id = register(&mut alice, "bob").await;
    let before = alice
        .fails("posts.create", r#"{"text": "без имени"}"#)
        .await;
    assert_eq!(
        before.code, "unauthorized",
        "пока соединение не представилось, действовать от чьего-то имени нельзя: {before:?}"
    );
    let me = alice.hello("Alice").await;
    assert_eq!(
        (me.id, me.username.as_str()),
        (alice_id, "alice"),
        "session.hello отвечает пользователем, ник нормализуется"
    );
    let post = ok!(alice, "posts.create", r#"{"text": "Привет из сети #tcp"}"# => PostView);
    assert_eq!(
        (post.author, post.text.as_str()),
        (alice_id, "Привет из сети #tcp"),
        "автор поста берётся из сессии, поле user в data больше не обязательно"
    );

    let mut bob = Peer::connect(gateway.addr).await;
    bob.hello("bob").await;
    let followed = ok!(bob, "users.follow", &format!(r#"{{"target": "{alice_id}"}}"#) => Changed);
    assert!(followed.changed, "bob подписался на alice");
    let feed = ok!(bob, "posts.feed", "{}" => FeedPage);
    assert_eq!(
        feed.posts
            .iter()
            .map(|post| post.id)
            .collect::<Vec<PostId>>(),
        vec![post.id],
        "в ленте bob пост alice"
    );
    let same = ok!(bob, "users.follow", &format!(r#"{{"user": "{bob_id}", "target": "{alice_id}"}}"#) => Changed);
    assert!(
        !same.changed,
        "свой собственный user в data разрешён, повторная подписка ничего не меняет"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn session_rules_are_enforced() {
    let gateway = Gateway::start(Config::default()).await;
    let mut alice = Peer::connect(gateway.addr).await;
    let alice_id = register(&mut alice, "alice").await;
    let bob_id = register(&mut alice, "bob").await;
    let unknown = alice
        .fails("session.hello", r#"{"username": "carol"}"#)
        .await;
    assert_eq!(
        unknown.code, "not_found",
        "представиться можно только существующим ником"
    );
    alice.hello("alice").await;
    let twice = alice.fails("session.hello", r#"{"username": "bob"}"#).await;
    assert_eq!(
        twice.code, "conflict",
        "второй hello без bye это конфликт: {twice:?}"
    );
    let stranger = alice
        .fails(
            "posts.create",
            &format!(r#"{{"user": "{bob_id}", "text": "я bob, честно"}}"#),
        )
        .await;
    assert_eq!(
        stranger.code, "forbidden",
        "от чужого имени нельзя: user в data должен совпадать с сессией"
    );
    let gone = ok!(alice, "session.bye", "{}" => UserView);
    assert_eq!(gone.id, alice_id, "bye отвечает тем, кто вышел");
    let again = alice.fails("session.bye", "{}").await;
    assert_eq!(again.code, "unauthorized", "bye без сессии это отказ");
    let found = ok!(alice, "users.find", r#"{"username": "bob"}"# => UserView);
    assert_eq!(found.id, bob_id, "искать можно и без сессии");
    let odd = alice.fails("session.dance", "{}").await;
    assert_eq!(odd.code, "unknown_method", "у сессии только hello и bye");
    gateway.stop().await;
}

#[tokio::test]
async fn pushes_reach_every_connection_of_the_user() {
    let gateway = Gateway::start(Config::default()).await;
    let mut phone = Peer::connect(gateway.addr).await;
    let alice = register(&mut phone, "alice").await;
    let bob = register(&mut phone, "bob").await;
    phone.hello("alice").await;
    let mut laptop = Peer::connect(gateway.addr).await;
    laptop.hello("alice").await;
    let mut other = Peer::connect(gateway.addr).await;
    other.hello("bob").await;

    let _ = ok!(other, "users.follow", &format!(r#"{{"target": "{alice}"}}"#) => Changed);
    let _ = ok!(other, "chat.send", format!(r#"{{"peer": "{alice}", "text": "привет"}}"#) => MessageView);
    for device in [&mut phone, &mut laptop] {
        assert_eq!(
            device.push().await,
            Push::Followed { to: alice, by: bob },
            "подписка приходит push во все соединения alice"
        );
        assert_eq!(
            device.push().await,
            Push::Message {
                to: alice,
                from: bob
            },
            "личное сообщение приходит push во все соединения alice"
        );
    }
    let history = ok!(phone, "chat.history", &format!(r#"{{"peer": "{bob}"}}"#) => ChatHistory);
    assert_eq!(history.messages.len(), 1, "сообщение лежит в истории");

    let post = ok!(laptop, "posts.create", r#"{"text": "Новый пост"}"# => PostView);
    assert_eq!(
        other.push().await,
        Push::NewPost {
            to: bob,
            author: alice,
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
    let mut peer = Peer::connect(gateway.addr).await;
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
    register(&mut peer, "alice").await;
    peer.raw(&"x".repeat(4096)).await;
    let Some(Line::Reply(raw)) = peer.line().await else {
        panic!("на слишком длинную строку шлюз отвечает ошибкой");
    };
    assert!(
        raw.contains("too_long"),
        "строка длиннее max_line это ошибка too_long: {raw}"
    );
    assert!(
        peer.closed().await,
        "после слишком длинной строки шлюз закрывает соединение: дочитывать её до конца никто не обязан"
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
    let mut busy = Peer::connect(gateway.addr).await;
    let mut idle = Peer::connect(gateway.addr).await;
    register(&mut busy, "alice").await;
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = ok!(busy, "users.find", r#"{"username": "alice"}"# => UserView);
    }
    assert!(
        idle.closed().await,
        "соединение без единой строки дольше idle_timeout шлюз закрывает"
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
    let mut first = Peer::connect(gateway.addr).await;
    register(&mut first, "alice").await;
    let mut second = Peer::connect(gateway.addr).await;
    let _ = ok!(second, "users.find", r#"{"username": "alice"}"# => UserView);
    let mut third = Peer::connect(gateway.addr).await;
    let id = third.send("users.find", r#"{"username": "alice"}"#).await;
    let early = timeout(Duration::from_millis(400), third.lines.next_line()).await;
    assert!(
        early.is_err(),
        "третье соединение сверх max_connections = 2 ждёт своей очереди и ответа пока не получает"
    );
    drop(first);
    let raw = within("ответ третьему", third.reply(id)).await;
    assert!(
        raw.contains("alice"),
        "как только место освободилось, третье соединение обслужено: {raw}"
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
    let path = journal("shutdown");
    let config = Config {
        journal: Some(path.clone()),
        ..Config::default()
    };
    let gateway = Gateway::start(config.clone()).await;
    let mut peer = Peer::connect(gateway.addr).await;
    register(&mut peer, "alice").await;
    peer.hello("alice").await;
    let _ = ok!(peer, "posts.create", r#"{"text": "До перезапуска"}"# => PostView);
    gateway.stop().await;
    assert!(
        peer.closed().await,
        "при остановке шлюз закрывает открытые соединения"
    );

    let gateway = Gateway::start(config).await;
    let mut peer = Peer::connect(gateway.addr).await;
    let me = peer.hello("alice").await;
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
    let addr = gateway.addr.to_string();
    let (mut alice_keys, alice_stdin) = tokio::io::duplex(4096);
    let (alice_stdout, alice_screen) = tokio::io::duplex(1 << 16);
    let alice = tokio::spawn({
        let addr = addr.clone();
        async move { social_cli::run(&addr, BufReader::new(alice_stdin), alice_stdout).await }
    });
    let mut screen = BufReader::new(alice_screen).lines();
    alice_keys
        .write_all("register alice\nhello alice\npost Привет\n".as_bytes())
        .await
        .expect("ввод alice пишется");
    read_until(&mut screen, "привет, @alice").await;
    read_until(&mut screen, "@alice [1] Привет (лайков: 0)").await;

    let mut bob_screen = Vec::new();
    within(
        "сеанс bob",
        social_cli::run(
            &addr,
            "register bob\nhello bob\nfollow alice\nfeed\nmsg alice как дела?\nquit\n".as_bytes(),
            &mut bob_screen,
        ),
    )
    .await
    .expect("сеанс bob проходит");
    let bob_screen = String::from_utf8(bob_screen).expect("клиент пишет UTF-8");
    assert!(
        bob_screen.contains("@alice [1] Привет (лайков: 0)") && bob_screen.contains("готово"),
        "bob подписался и видит пост alice в ленте:\n{bob_screen}"
    );
    let seen = read_until(&mut screen, ": как дела?").await;
    assert!(
        seen.iter()
            .any(|line| line.starts_with("* на тебя подписался ")),
        "alice видит push о подписке: {seen:?}"
    );
    assert!(
        seen.last().is_some_and(|line| line.starts_with("* ")),
        "push о сообщении клиент дополняет текстом из истории: {seen:?}"
    );
    alice_keys
        .write_all(b"quit\n")
        .await
        .expect("ввод alice пишется");
    within("сеанс alice", alice)
        .await
        .expect("задача клиента не падает")
        .expect("сеанс alice проходит");
    gateway.stop().await;
}
