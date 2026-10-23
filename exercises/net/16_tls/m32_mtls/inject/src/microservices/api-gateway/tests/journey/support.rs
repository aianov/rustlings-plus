use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use api_gateway::{Config, Routes, Services, run_with};
use contracts::{Changed, ErrorBody, FeedPush, Login, Outcome, Push, Response, UserView, decode};
use grpc_kit::{LogSink, Tls};
use service_runtime::CancellationToken;
use social_cli::{Dialer, Web, Ws};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub const DEADLINE: Duration = Duration::from_secs(10);

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
        }
    }
}

type Task = JoinHandle<Result<(), String>>;

pub struct Gateway {
    pub http: SocketAddr,
    pub auth: SocketAddr,
    pub services: Services,
    pub dialer: Dialer,
    pub certs: Option<PathBuf>,
    stop: CancellationToken,
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
        Gateway::boot(config, routes, None, None, None).await
    }

    pub async fn with_journals(config: Config, journals: &Journals) -> Gateway {
        Gateway::boot(config, Routes::social(), Some(journals), None, None).await
    }

    pub async fn logged(config: Config, log: LogSink) -> Gateway {
        Gateway::boot(config, Routes::social(), None, Some(log), None).await
    }

    pub async fn secure(name: &str, config: Config) -> Gateway {
        let certs = issue_certs(name);
        Gateway::boot(config, Routes::social(), None, None, Some(certs)).await
    }

    async fn boot(
        mut config: Config,
        routes: Routes,
        journals: Option<&Journals>,
        log: Option<LogSink>,
        certs: Option<PathBuf>,
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
        let users_config = user_service::Config {
            journal: journals.map(|j| j.users.clone()),
            log: log.clone(),
            tls: tls(certs.as_ref(), "user-service"),
            ..user_service::Config::default()
        };
        let posts_config = post_service::Config {
            users: users_addr.to_string(),
            journal: journals.map(|j| j.posts.clone()),
            log: log.clone(),
            tls: tls(certs.as_ref(), "post-service"),
            ..post_service::Config::default()
        };
        let chat_config = chat_service::Config {
            users: users_addr.to_string(),
            journal: journals.map(|j| j.chat.clone()),
            log: log.clone(),
            tls: tls(certs.as_ref(), "chat-service"),
            ..chat_service::Config::default()
        };
        let auth_config = auth_service::Config {
            users: users_addr.to_string(),
            journal: journals.map(|j| j.auth.clone()),
            log,
            tls: tls(certs.as_ref(), "auth-service"),
            ..auth_service::Config::default()
        };
        config.services = Services {
            auth: auth_grpc_addr.to_string(),
            users: users_addr.to_string(),
            posts: posts_addr.to_string(),
            chat: chat_addr.to_string(),
        };
        config.tls = tls(certs.as_ref(), "api-gateway");
        let services = config.services.clone();
        let auth_task = auth_service::run(auth_http, auth_grpc, auth_config, stop.clone());
        let tasks = vec![
            (
                "api-gateway",
                spawn(run_with(http, config, routes, stop.clone())),
            ),
            ("auth-service", spawn(auth_task)),
            (
                "chat-service",
                spawn(chat_service::run(chat, chat_config, stop.clone())),
            ),
            (
                "post-service",
                spawn(post_service::run(posts, posts_config, stop.clone())),
            ),
            (
                "user-service",
                spawn(user_service::run(users, users_config, stop.clone())),
            ),
        ];
        let gateway = Gateway {
            http: addr,
            auth,
            services,
            dialer,
            certs,
            stop,
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

    pub fn auth_web(&self) -> Web {
        Web::with(&self.auth.to_string(), self.dialer.clone())
    }

    pub async fn register(&self, name: &str) -> UserView {
        let body = format!(r#"{{"username": "{name}"}}"#);
        let answer = within(
            "регистрация",
            self.auth_web()
                .request("POST", "/auth/register", Some(&body), None),
        )
        .await
        .unwrap_or_else(|e| panic!("регистрация {name}: {e}"));
        assert_eq!(answer.status, 201, "регистрация {name}: {answer:?}");
        decode(&answer.body)
            .unwrap_or_else(|e| panic!("ответ регистрации разбирается ({e}): {answer:?}"))
    }

    pub async fn login(&self, name: &str) -> Login {
        let body = format!(r#"{{"username": "{name}"}}"#);
        let answer = within(
            "вход",
            self.auth_web()
                .request("POST", "/auth/login", Some(&body), None),
        )
        .await
        .unwrap_or_else(|e| panic!("вход {name}: {e}"));
        assert_eq!(answer.status, 200, "вход {name}: {answer:?}");
        decode(&answer.body).unwrap_or_else(|e| panic!("ответ входа разбирается ({e}): {answer:?}"))
    }

    pub async fn peer_with(&self, token: &str) -> Peer {
        Peer::connect_with(&self.dialer, self.http, &format!("/ws?token={token}")).await
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
