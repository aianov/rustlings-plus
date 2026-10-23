use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use api_gateway::{Config, Routes, Services, run_with};
use contracts::{Login, Topics, UserView, decode};
use service_runtime::CancellationToken;
use social_cli::{Reply, Web};
use test_support::{EVENTUALLY, NewUser, unique_root};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::brokers::{drop_streams, nats};
use crate::client::Client;
use crate::waiting::eventually;

type Task = JoinHandle<Result<(), String>>;

#[derive(Debug, Clone, Default)]
pub struct StackConfig {
    pub gateway: Config,
    pub digest_every: Option<Duration>,
}

pub struct Stack {
    pub http: SocketAddr,
    pub auth: SocketAddr,
    pub services: Services,
    pub topics: Topics,
    stop: CancellationToken,
    tasks: Vec<(&'static str, Task)>,
}

async fn listener() -> (TcpListener, SocketAddr) {
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

impl Stack {
    pub async fn start() -> Stack {
        Stack::start_with(StackConfig::default()).await
    }

    pub async fn start_with(base: StackConfig) -> Stack {
        let stop = CancellationToken::new();
        let (users, users_addr) = listener().await;
        let (posts, posts_addr) = listener().await;
        let (chat, chat_addr) = listener().await;
        let (auth_http, auth) = listener().await;
        let (auth_grpc, auth_grpc_addr) = listener().await;
        let (http, addr) = listener().await;
        let (feed, feed_addr) = listener().await;
        let topics = Topics::new(&unique_root("e2e"));
        let files = std::env::temp_dir().join("e2e-files").join(topics.root());
        let users_config = user_service::Config {
            nats: Some(nats("user-service")),
            topics: topics.clone(),
            files,
            ..user_service::Config::default()
        };
        let posts_config = post_service::Config {
            users: users_addr.to_string(),
            nats: Some(nats("post-service")),
            topics: topics.clone(),
            ..post_service::Config::default()
        };
        let feed_config = feed_service::Config {
            nats: nats("feed-service"),
            topics: topics.clone(),
            ..feed_service::Config::default()
        };
        let notify_config = notify_service::Config {
            users: users_addr.to_string(),
            nats: nats("notify-service"),
            topics: topics.clone(),
            digest_every: base
                .digest_every
                .unwrap_or(notify_service::DEFAULT_DIGEST_EVERY),
            ..notify_service::Config::default()
        };
        let chat_config = chat_service::Config {
            users: users_addr.to_string(),
            nats: Some(nats("chat-service")),
            topics: topics.clone(),
            ..chat_service::Config::default()
        };
        let auth_config = auth_service::Config {
            users: users_addr.to_string(),
            posts: posts_addr.to_string(),
            nats: Some(nats("auth-service")),
            topics: topics.clone(),
            ..auth_service::Config::default()
        };
        let services = Services {
            auth: auth_grpc_addr.to_string(),
            auth_http: auth.to_string(),
            users: users_addr.to_string(),
            posts: posts_addr.to_string(),
            chat: chat_addr.to_string(),
            feed: feed_addr.to_string(),
        };
        let gateway = Config {
            services: services.clone(),
            nats: Some(nats("api-gateway")),
            topics: topics.clone(),
            ..base.gateway
        };
        let notify = notify_service::start(None, notify_config, stop.child())
            .await
            .expect("notify-service подписался на события: NATS курса запущен?");
        let tasks = vec![
            (
                "api-gateway",
                spawn(run_with(http, gateway, Routes::social(), stop.clone())),
            ),
            (
                "auth-service",
                spawn(auth_service::run(
                    auth_http,
                    auth_grpc,
                    auth_config,
                    stop.child(),
                )),
            ),
            (
                "chat-service",
                spawn(chat_service::run(chat, chat_config, stop.child())),
            ),
            (
                "post-service",
                spawn(post_service::run(posts, posts_config, stop.child())),
            ),
            (
                "user-service",
                spawn(user_service::run(users, users_config, stop.child())),
            ),
            ("notify-service", spawn(notify.finished())),
            (
                "feed-service",
                spawn(feed_service::run(feed, feed_config, stop.child())),
            ),
        ];
        let stack = Stack {
            http: addr,
            auth,
            services,
            topics,
            stop,
            tasks,
        };
        stack.ready().await;
        stack
    }

    pub fn web(&self) -> Web {
        Web::new(&self.http.to_string())
    }

    pub async fn ready(&self) {
        eventually(
            "готовность стека: /health шлюза ждёт все сервисы",
            || async {
                let reply = self
                    .web()
                    .request("GET", "/health", None, None)
                    .await
                    .map_err(|error| error.to_string())?;
                match reply.status {
                    200 => Ok(()),
                    status => Err(format!("HTTP {status}: {}", reply.body)),
                }
            },
        )
        .await;
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        token: Option<&str>,
    ) -> Reply {
        timeout(EVENTUALLY, self.web().request(method, path, body, token))
            .await
            .unwrap_or_else(|_| panic!("{method} {path}: шлюз молчит"))
            .unwrap_or_else(|error| panic!("{method} {path}: {error}"))
    }

    pub async fn register(&self, user: &NewUser) -> UserView {
        let reply = self
            .request("POST", "/auth/register", Some(&user.credentials()), None)
            .await;
        assert_eq!(
            reply.status, 201,
            "регистрация {}: {reply:?}",
            user.username
        );
        decode(&reply.body).unwrap_or_else(|e| panic!("ответ регистрации ({e}): {reply:?}"))
    }

    pub async fn login(&self, user: &NewUser) -> Login {
        let reply = self
            .request("POST", "/auth/login", Some(&user.credentials()), None)
            .await;
        assert_eq!(reply.status, 200, "вход {}: {reply:?}", user.username);
        decode(&reply.body).unwrap_or_else(|e| panic!("ответ входа ({e}): {reply:?}"))
    }

    pub async fn connect(&self, token: &str) -> Client {
        Client::connect(self.http, &format!("/ws?token={token}")).await
    }

    pub async fn member(&self, user: &NewUser) -> (UserView, Client) {
        let view = self.register(user).await;
        let login = self.login(user).await;
        (view, self.connect(&login.token).await)
    }

    pub async fn stop(self) {
        self.stop.cancel();
        for (name, task) in self.tasks {
            let finished = timeout(EVENTUALLY, task)
                .await
                .unwrap_or_else(|_| panic!("{name} не остановился за {EVENTUALLY:?}"))
                .unwrap_or_else(|error| panic!("задача {name} упала: {error}"));
            if let Err(error) = finished {
                panic!("{name}: run завершился ошибкой: {error}");
            }
        }
        drop_streams(&self.topics).await;
    }
}
