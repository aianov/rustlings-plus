use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use api_gateway::{Config, Routes, Services, run_with, serve};
use contracts::{
    Changed, ErrorBody, FeedPage, FeedPush, Login, Outcome, Push, Response, Topics, UserId,
    UserView, decode,
};
use grpc_kit::{LogSink, Tls};
use nats_engine::{BusConfig, DEFAULT_NATS_URL};
use service_runtime::CancellationToken;
use social_cli::{Dialer, Web, Ws};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub const DEADLINE: Duration = Duration::from_secs(10);

pub const NATS_ENV: &str = "RUSTLINGS_NATS_URL";

pub fn nats_url() -> String {
    std::env::var(NATS_ENV)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

pub fn nats(name: &str) -> BusConfig {
    BusConfig::new(nats_url()).named(name)
}

pub async fn drop_stream(topics: &Topics) {
    let kv = |bucket: String| format!("KV_{bucket}");
    let names = [
        topics.stream(),
        topics.chat_stream(),
        topics.jobs_stream(),
        topics.jobs_dead_stream(),
        kv(topics.presence()),
        kv(topics.checkpoints()),
        kv(topics.sagas()),
        kv(format!("{}_SWEEP", topics.leases())),
        kv(format!("{}_DIGEST", topics.leases())),
        kv(format!("{}_ORPHANS", topics.leases())),
    ];
    for name in names {
        drop_one(&name).await;
    }
}

async fn drop_one(name: &str) {
    for _ in 0..50 {
        if let Ok(client) = async_nats::connect(nats_url()).await {
            let js = async_nats::jetstream::new(client);
            let _ = js.delete_stream(name).await;
            match js.get_stream(name).await {
                Err(error) if gone(&error) => return,
                Ok(..) | Err(..) => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("stream {name} не удаляется: тест оставит мусор в брокере курса");
}

fn gone(error: &async_nats::jetstream::context::GetStreamError) -> bool {
    match error.kind() {
        async_nats::jetstream::context::GetStreamErrorKind::JetStream(inner) => {
            inner.error_code() == async_nats::jetstream::ErrorCode::STREAM_NOT_FOUND
        }
        _ => false,
    }
}

pub fn plain(push: Push) -> Push {
    match push {
        Push::Message { to, from, text, .. } => Push::Message {
            to,
            from,
            seq: 0,
            text,
        },
        other => other,
    }
}

pub fn letter(to: UserId, from: UserId, text: &str) -> Push {
    Push::Message {
        to,
        from,
        seq: 0,
        text: text.to_string(),
    }
}

pub fn fresh_topics() -> Topics {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
    Topics::new(&format!(
        "t{}_{nanos:x}_{serial}",
        std::process::id() % 100_000
    ))
}

pub async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

pub fn id_of(raw: &str) -> Option<u64> {
    let rest = raw.strip_prefix(r#"{"id":"#)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub enum Line {
    Push(Push),
    Feed(FeedPush),
    Reply(String),
}

macro_rules! ok {
    ($peer:expr, $method:expr, $data:expr => $ty:ty) => {{
        let method = $method;
        let data = $data;
        let raw = $peer.call(method, &data).await;
        match contracts::decode::<contracts::Response<$ty>>(&raw) {
            Ok(contracts::Response {
                outcome: contracts::Outcome::Ok(value),
                ..
            }) => value,
            Ok(contracts::Response {
                outcome: contracts::Outcome::Error(body),
                ..
            }) => panic!("{method} {data}: ждали успех, а пришло {body:?}"),
            Err(error) => panic!("ответ на {method} не разобрался ({error}): {raw}"),
        }
    }};
}

pub(crate) use ok;

#[derive(Debug, Clone)]
pub struct Journals {
    pub users: PathBuf,
    pub posts: PathBuf,
    pub auth: PathBuf,
    pub chat: PathBuf,
    pub topics: Topics,
}

impl Journals {
    pub fn fresh(milestone: &str, name: &str) -> Journals {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(milestone)
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        Journals {
            users: dir.join("users.ndjson"),
            posts: dir.join("posts.ndjson"),
            auth: dir.join("auth.ndjson"),
            chat: dir.join("chat.ndjson"),
            topics: fresh_topics(),
        }
    }

    pub async fn forget(self) {
        drop_stream(&self.topics).await;
    }
}

type Task = JoinHandle<Result<(), String>>;

pub struct Gateway {
    pub http: SocketAddr,
    pub auth: SocketAddr,
    pub services: Services,
    pub dialer: Dialer,
    pub certs: Option<PathBuf>,
    pub admin: Option<SocketAddr>,
    pub topics: Topics,
    keep_stream: bool,
    stop: CancellationToken,
    halts: Vec<(&'static str, CancellationToken)>,
    tasks: Vec<(&'static str, Task)>,
}

pub fn issue_certs(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("certs")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    certgen::issue_all(&dir, false).expect("сертификаты для теста выпускаются");
    dir
}

fn tls(certs: Option<&PathBuf>, name: &str) -> Option<Tls> {
    certs.map(|dir| Tls::load(dir, name).expect("сертификаты сервиса читаются"))
}

pub async fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 на 127.0.0.1 свободен всегда");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    (listener, addr)
}

fn spawn<E, F>(work: F) -> Task
where
    E: std::fmt::Debug,
    F: Future<Output = Result<(), E>> + Send + 'static,
{
    tokio::spawn(async move { work.await.map_err(|error| format!("{error:?}")) })
}

impl Gateway {
    pub async fn start(config: Config) -> Gateway {
        Gateway::start_with(config, Routes::social()).await
    }

    pub async fn start_with(config: Config, routes: Routes) -> Gateway {
        Gateway::boot(config, Some(routes), None, None, None, None).await
    }

    pub async fn with_journals(config: Config, journals: &Journals) -> Gateway {
        Gateway::boot(
            config,
            Some(Routes::social()),
            Some(journals),
            None,
            None,
            None,
        )
        .await
    }

    pub async fn logged(config: Config, log: LogSink) -> Gateway {
        Gateway::boot(config, Some(Routes::social()), None, Some(log), None, None).await
    }

    pub async fn secure(name: &str, config: Config) -> Gateway {
        let certs = issue_certs(name);
        Gateway::boot(
            config,
            Some(Routes::social()),
            None,
            None,
            Some(certs),
            None,
        )
        .await
    }

    pub async fn with_admin(config: Config) -> Gateway {
        Gateway::boot(config, None, None, None, None, None).await
    }

    pub async fn with_auth(config: Config, auth: auth_service::Config) -> Gateway {
        Gateway::boot(config, None, None, None, None, Some(auth)).await
    }

    pub async fn custom(config: Config, routes: Routes, auth: auth_service::Config) -> Gateway {
        Gateway::boot(config, Some(routes), None, None, None, Some(auth)).await
    }

    pub async fn with_digest(config: Config, every: Duration) -> Gateway {
        let routes = Some(Routes::social());
        Gateway::boot_full(config, routes, None, None, None, None, Some(every)).await
    }

    async fn boot(
        config: Config,
        routes: Option<Routes>,
        journals: Option<&Journals>,
        log: Option<LogSink>,
        certs: Option<PathBuf>,
        auth_base: Option<auth_service::Config>,
    ) -> Gateway {
        Gateway::boot_full(config, routes, journals, log, certs, auth_base, None).await
    }

    async fn boot_full(
        mut config: Config,
        routes: Option<Routes>,
        journals: Option<&Journals>,
        log: Option<LogSink>,
        certs: Option<PathBuf>,
        auth_base: Option<auth_service::Config>,
        digest: Option<Duration>,
    ) -> Gateway {
        let dialer = match &certs {
            Some(dir) => {
                let ca = std::fs::read(dir.join("ca.pem")).expect("ca.pem");
                Dialer::trusting(&ca).expect("клиент доверяет нашему CA")
            }
            None => Dialer::plain(),
        };
        let stop = CancellationToken::new();
        let (users, users_addr) = listener().await;
        let (posts, posts_addr) = listener().await;
        let (chat, chat_addr) = listener().await;
        let (auth_http, auth) = listener().await;
        let (auth_grpc, auth_grpc_addr) = listener().await;
        let (http, addr) = listener().await;
        let (feed, feed_addr) = listener().await;
        let topics = journals.map_or_else(fresh_topics, |j| j.topics.clone());
        let users_config = user_service::Config {
            journal: journals.map(|j| j.users.clone()),
            log: log.clone(),
            tls: tls(certs.as_ref(), "user-service"),
            nats: Some(nats("user-service")),
            topics: topics.clone(),
            ..user_service::Config::default()
        };
        let posts_config = post_service::Config {
            users: users_addr.to_string(),
            journal: journals.map(|j| j.posts.clone()),
            log: log.clone(),
            tls: tls(certs.as_ref(), "post-service"),
            nats: Some(nats("post-service")),
            topics: topics.clone(),
            ..post_service::Config::default()
        };
        let feed_config = feed_service::Config {
            nats: nats("feed-service"),
            topics: topics.clone(),
            tls: tls(certs.as_ref(), "feed-service"),
            log: log.clone(),
            ..feed_service::Config::default()
        };
        let notify_config = notify_service::Config {
            users: users_addr.to_string(),
            nats: nats("notify-service"),
            topics: topics.clone(),
            tls: tls(certs.as_ref(), "notify-service"),
            digest_every: digest.unwrap_or(notify_service::DEFAULT_DIGEST_EVERY),
            ..notify_service::Config::default()
        };
        let chat_config = chat_service::Config {
            users: users_addr.to_string(),
            journal: journals.map(|j| j.chat.clone()),
            log: log.clone(),
            tls: tls(certs.as_ref(), "chat-service"),
            nats: Some(nats("chat-service")),
            topics: topics.clone(),
            ..chat_service::Config::default()
        };
        let auth_config = auth_service::Config {
            users: users_addr.to_string(),
            posts: posts_addr.to_string(),
            journal: journals.map(|j| j.auth.clone()),
            log,
            tls: tls(certs.as_ref(), "auth-service"),
            nats: Some(nats("auth-service")),
            topics: topics.clone(),
            ..auth_base.unwrap_or_default()
        };
        config.services = Services {
            auth: auth_grpc_addr.to_string(),
            auth_http: auth.to_string(),
            users: users_addr.to_string(),
            posts: posts_addr.to_string(),
            chat: chat_addr.to_string(),
            feed: feed_addr.to_string(),
        };
        config.tls = tls(certs.as_ref(), "api-gateway");
        config.nats = Some(nats("api-gateway"));
        config.topics = topics.clone();
        let services = config.services.clone();
        let halts: Vec<(&'static str, CancellationToken)> = [
            "auth-service",
            "chat-service",
            "post-service",
            "user-service",
            "notify-service",
            "feed-service",
        ]
        .into_iter()
        .map(|name| (name, stop.child()))
        .collect();
        let halt = |name: &str| {
            halts
                .iter()
                .find(|(own, _)| *own == name)
                .map(|(_, token)| token.clone())
                .expect("сервис из списка")
        };
        let auth_task = auth_service::run(auth_http, auth_grpc, auth_config, halt("auth-service"));
        let notify = notify_service::start(None, notify_config, halt("notify-service"))
            .await
            .expect("notify-service подписался на события: NATS курса запущен?");
        let (gateway_task, admin) = match routes {
            Some(routes) => (spawn(run_with(http, config, routes, stop.clone())), None),
            None => {
                let (admin, admin_addr) = listener().await;
                (
                    spawn(serve(http, admin, config, stop.clone())),
                    Some(admin_addr),
                )
            }
        };
        let tasks = vec![
            ("api-gateway", gateway_task),
            ("auth-service", spawn(auth_task)),
            (
                "chat-service",
                spawn(chat_service::run(chat, chat_config, halt("chat-service"))),
            ),
            (
                "post-service",
                spawn(post_service::run(posts, posts_config, halt("post-service"))),
            ),
            (
                "user-service",
                spawn(user_service::run(users, users_config, halt("user-service"))),
            ),
            ("notify-service", spawn(notify.finished())),
            (
                "feed-service",
                spawn(feed_service::run(feed, feed_config, halt("feed-service"))),
            ),
        ];
        let gateway = Gateway {
            http: addr,
            auth,
            services,
            dialer,
            certs,
            admin,
            topics,
            keep_stream: journals.is_some(),
            stop,
            halts,
            tasks,
        };
        gateway.ready().await;
        gateway
    }

    pub async fn ready(&self) {
        within(
            "готовность стека: /health шлюза ждёт SERVING всех сервисов",
            async {
                let mut web = self.web();
                loop {
                    if let Ok(reply) = web.request("GET", "/health", None, None).await {
                        if reply.status == 200 {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            },
        )
        .await;
    }

    pub async fn halt(&mut self, name: &str) {
        if let Some((_, token)) = self.halts.iter().find(|(own, _)| *own == name) {
            token.cancel();
        }
        let at = self
            .tasks
            .iter()
            .position(|(own, _)| *own == name)
            .unwrap_or_else(|| panic!("{name} не запущен"));
        let (_, task) = self.tasks.remove(at);
        within(&format!("остановка {name}"), task)
            .await
            .unwrap_or_else(|e| panic!("задача {name} упала: {e}"))
            .unwrap_or_else(|e| panic!("{name}: run завершился ошибкой: {e}"));
    }

    pub async fn stop(self) {
        self.stop.cancel();
        for (name, task) in self.tasks {
            let finished = within(&format!("остановка {name}"), task)
                .await
                .unwrap_or_else(|e| panic!("задача {name} упала: {e}"));
            if let Err(error) = finished {
                panic!("{name}: run завершился ошибкой: {error}");
            }
        }
        if !self.keep_stream {
            drop_stream(&self.topics).await;
        }
    }

    pub async fn feed_until(
        &self,
        peer: &mut Peer,
        what: &str,
        done: impl Fn(&FeedPage) -> bool,
    ) -> FeedPage {
        within(
            &format!("лента догоняет события: {what}"),
            async {
                loop {
                    let raw = peer.call("posts.feed", "{}").await;
                    if let Ok(Response {
                        outcome: Outcome::Ok(page),
                        ..
                    }) = decode::<Response<FeedPage>>(&raw)
                    {
                        if done(&page) {
                            return page;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            },
        )
        .await
    }

    pub async fn auth_http(&self, method: &str, path: &str, body: Option<&str>) -> Http {
        http(self.auth, method, path, body).await
    }

    pub async fn http(&self, method: &str, path: &str, body: Option<&str>) -> Http {
        http(self.http, method, path, body).await
    }

    pub fn web(&self) -> Web {
        Web::with(&self.http.to_string(), self.dialer.clone())
    }

    pub async fn register(&self, name: &str) -> UserView {
        let body = format!(r#"{{"username": "{name}", "password": "correct-horse"}}"#);
        let answer = within(
            "регистрация через шлюз",
            self.web()
                .request("POST", "/auth/register", Some(&body), None),
        )
        .await
        .unwrap_or_else(|e| panic!("регистрация {name}: {e}"));
        assert_eq!(answer.status, 201, "регистрация {name}: {answer:?}");
        decode(&answer.body)
            .unwrap_or_else(|e| panic!("ответ регистрации разбирается ({e}): {answer:?}"))
    }

    pub async fn login(&self, name: &str) -> Login {
        let body = format!(r#"{{"username": "{name}", "password": "correct-horse"}}"#);
        let answer = within(
            "вход через шлюз",
            self.web().request("POST", "/auth/login", Some(&body), None),
        )
        .await
        .unwrap_or_else(|e| panic!("вход {name}: {e}"));
        assert_eq!(answer.status, 200, "вход {name}: {answer:?}");
        decode(&answer.body).unwrap_or_else(|e| panic!("ответ входа разбирается ({e}): {answer:?}"))
    }

    pub async fn peer_with(&self, token: &str) -> Peer {
        Peer::connect_with(&self.dialer, self.http, &format!("/ws?token={token}")).await
    }

    pub async fn peer_after(&self, token: &str, after: u64) -> Peer {
        let path = format!("/ws?token={token}&after={after}");
        Peer::connect_with(&self.dialer, self.http, &path).await
    }

    pub async fn twin(&self, base: Config) -> Twin {
        let (http, addr) = listener().await;
        let config = Config {
            services: self.services.clone(),
            nats: Some(nats("api-gateway-2")),
            topics: self.topics.clone(),
            ..base
        };
        let stop = self.stop.child();
        let task = spawn(run_with(http, config, Routes::social(), stop.clone()));
        let twin = Twin {
            http: addr,
            stop,
            task,
        };
        within("готовность второго шлюза", async {
            loop {
                if http_get_ok(addr).await {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        twin
    }

    pub async fn device(&self, name: &str) -> Peer {
        let login = self.login(name).await;
        self.peer_with(&login.token).await
    }

    pub async fn member(&self, name: &str) -> (UserView, Peer) {
        let user = self.register(name).await;
        let peer = self.device(name).await;
        (user, peer)
    }

    pub async fn refused(&self, path: &str) -> String {
        match within("отказ WebSocket", Ws::connect(&self.http.to_string(), path)).await {
            Ok(..) => panic!("шлюз открыл WebSocket на {path}, а должен был отказать до 101"),
            Err(error) => error.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Http {
    pub status: u16,
    pub head: String,
    pub body: String,
}

impl Http {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.split("\r\n").skip(1).find_map(|line| {
            let (own, value) = line.split_once(':')?;
            own.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

pub async fn http(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> Http {
    let mut stream = within("подключение к HTTP", TcpStream::connect(addr))
        .await
        .expect("HTTP-порт шлюза принимает соединения");
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    if let Some(body) = body {
        raw.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
    } else {
        raw.push_str("\r\n");
    }
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
    Http {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

pub struct Twin {
    pub http: SocketAddr,
    stop: CancellationToken,
    task: Task,
}

async fn http_get_ok(addr: SocketAddr) -> bool {
    let reply = timeout(Duration::from_secs(1), async {
        Web::new(&addr.to_string())
            .request("GET", "/health", None, None)
            .await
    })
    .await;
    matches!(reply, Ok(Ok(reply)) if reply.status == 200)
}

impl Twin {
    pub async fn peer_with(&self, token: &str) -> Peer {
        Peer::connect(self.http, &format!("/ws?token={token}")).await
    }

    pub async fn peer_after(&self, token: &str, after: u64) -> Peer {
        Peer::connect(self.http, &format!("/ws?token={token}&after={after}")).await
    }

    pub async fn refused(&self, path: &str) -> String {
        match within("отказ WebSocket", Ws::connect(&self.http.to_string(), path)).await {
            Ok(..) => panic!("второй шлюз открыл WebSocket на {path}, а должен был отказать"),
            Err(error) => error.to_string(),
        }
    }

    pub async fn stop(self) {
        self.stop.cancel();
        let finished = within("остановка второго шлюза", self.task)
            .await
            .unwrap_or_else(|e| panic!("задача второго шлюза упала: {e}"));
        if let Err(error) = finished {
            panic!("второй шлюз: run завершился ошибкой: {error}");
        }
    }
}

pub struct Peer {
    ws: Ws,
    next: u64,
    pushes: VecDeque<Push>,
    feeds: VecDeque<FeedPush>,
}

impl Peer {
    pub async fn connect(addr: SocketAddr, path: &str) -> Peer {
        Peer::connect_with(&Dialer::plain(), addr, path).await
    }

    pub async fn connect_with(dialer: &Dialer, addr: SocketAddr, path: &str) -> Peer {
        let ws = within(
            "подключение WebSocket",
            Ws::connect_with(dialer, &addr.to_string(), path),
        )
        .await
        .unwrap_or_else(|e| panic!("шлюз открывает WebSocket на {path}: {e}"));
        Peer {
            ws,
            next: 0,
            pushes: VecDeque::new(),
            feeds: VecDeque::new(),
        }
    }

    pub async fn raw(&mut self, line: &str) {
        self.ws
            .send_text(line)
            .await
            .expect("текстовый фрейм уходит в шлюз");
    }

    pub async fn binary(&mut self, bytes: &[u8]) {
        self.ws
            .send_binary(bytes)
            .await
            .expect("двоичный фрейм уходит в шлюз");
    }

    pub fn close_code(&self) -> Option<u16> {
        self.ws.close_code()
    }

    pub async fn text(&mut self) -> Option<String> {
        within("сообщение от шлюза", self.ws.recv())
            .await
            .ok()
            .flatten()
    }

    pub async fn line(&mut self) -> Option<Line> {
        let raw = self.text().await?;
        if let Ok(push) = decode::<Push>(&raw) {
            return Some(Line::Push(push));
        }
        match decode::<FeedPush>(&raw) {
            Ok(feed) => Some(Line::Feed(feed)),
            Err(_) => Some(Line::Reply(raw)),
        }
    }

    pub async fn send(&mut self, method: &str, data: &str) -> u64 {
        self.next += 1;
        let service = method.split('.').next().unwrap_or(method);
        let line = format!(
            r#"{{"id": {}, "service": "{service}", "method": "{method}", "data": {data}}}"#,
            self.next
        );
        self.raw(&line).await;
        self.next
    }

    pub async fn reply(&mut self, id: u64) -> String {
        loop {
            match self.line().await {
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Feed(feed)) => self.feeds.push_back(feed),
                Some(Line::Reply(raw)) if id_of(&raw) == Some(id) => return raw,
                Some(Line::Reply(raw)) => panic!("ждали ответ на запрос {id}, а пришло {raw}"),
                None => panic!("шлюз закрыл соединение, не ответив на запрос {id}"),
            }
        }
    }

    pub async fn any_reply(&mut self) -> String {
        loop {
            match self.line().await {
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Feed(feed)) => self.feeds.push_back(feed),
                Some(Line::Reply(raw)) => return raw,
                None => panic!("шлюз закрыл соединение, не ответив на запросы"),
            }
        }
    }

    pub async fn call(&mut self, method: &str, data: &str) -> String {
        let id = self.send(method, data).await;
        self.reply(id).await
    }

    pub async fn fails(&mut self, method: &str, data: &str) -> ErrorBody {
        let raw = self.call(method, data).await;
        match decode::<Response<Changed>>(&raw) {
            Ok(Response {
                outcome: Outcome::Error(body),
                ..
            }) => body,
            _ => panic!("{method} {data}: ждали ошибку, а пришло {raw}"),
        }
    }

    pub async fn push(&mut self) -> Push {
        if let Some(push) = self.pushes.pop_front() {
            return push;
        }
        loop {
            match self.line().await {
                Some(Line::Push(push)) => return push,
                Some(Line::Feed(feed)) => self.feeds.push_back(feed),
                Some(Line::Reply(raw)) => panic!("ждали push, а пришёл ответ {raw}"),
                None => panic!("шлюз закрыл соединение, а push так и не пришёл"),
            }
        }
    }

    pub async fn quiet(&mut self, wait: Duration) -> Option<String> {
        if let Some(push) = self.pushes.pop_front() {
            return Some(format!("{push:?}"));
        }
        match timeout(wait, self.ws.recv()).await {
            Ok(Ok(Some(text))) => Some(text),
            Ok(Ok(None) | Err(..)) | Err(..) => None,
        }
    }

    pub fn pending_feeds(&self) -> usize {
        self.feeds.len()
    }

    pub async fn feed(&mut self) -> FeedPush {
        if let Some(feed) = self.feeds.pop_front() {
            return feed;
        }
        loop {
            match self.line().await {
                Some(Line::Feed(feed)) => return feed,
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Reply(raw)) => panic!("ждали событие ленты, а пришёл ответ {raw}"),
                None => panic!("шлюз закрыл соединение, а событие ленты так и не пришло"),
            }
        }
    }

    pub async fn closed(&mut self) -> bool {
        self.closed_slowly(Duration::ZERO).await.is_some()
    }

    pub async fn closed_slowly(&mut self, pause: Duration) -> Option<usize> {
        let mut seen = 0;
        loop {
            match timeout(DEADLINE, self.ws.recv()).await {
                Ok(Ok(Some(..))) => seen += 1,
                Ok(Ok(None) | Err(..)) => return Some(seen),
                Err(..) => return None,
            }
            if !pause.is_zero() {
                tokio::time::sleep(pause).await;
            }
        }
    }
}
