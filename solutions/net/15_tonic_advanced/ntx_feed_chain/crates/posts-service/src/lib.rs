// Лента через цепочку: токен, request-id, бюджет времени, стрим, здоровье и остановка.
// Заготовка даёт каркас; пробелы, помеченные todo!, заполняешь ты (список в README проекта).
mod log_layer;

pub use log_layer::{LogLayer, LogLine, Sink};
use proto::feed::v1::feed_server::{Feed, FeedServer};
use proto::feed::v1::{Item, PublishReply, PublishRequest, TimelineRequest};
use proto::users::v1::users_client::UsersClient;
use proto::users::v1::{GetUserRequest, User};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::service::Interceptor;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};
use tonic_health::server::health_reporter;
use tonic_health::ServingStatus;

pub const SERVICE: &str = "feed.v1.Feed";
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(2);

pub struct Config {
    pub tokens: Vec<(String, u64)>,
    pub grace: Duration,
    pub sink: Sink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(pub u64);

#[derive(Clone)]
struct Auth {
    tokens: Arc<HashMap<String, u64>>,
}

impl Interceptor for Auth {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let denied = || Status::unauthenticated("нужен действующий токен");
        let header = request.metadata().get("authorization").ok_or_else(denied)?;
        let header = header.to_str().map_err(|_| denied())?;
        let token = header.strip_prefix("Bearer ").ok_or_else(denied)?;
        let id = *self.tokens.get(token).ok_or_else(denied)?;
        request.extensions_mut().insert(UserId(id));
        Ok(request)
    }
}

#[derive(Clone)]
struct StoredPost {
    id: u64,
    author: u64,
    text: String,
}

pub fn parse_grpc_timeout(value: &str) -> Option<Duration> {
    let unit = value.chars().last()?;
    let digits = &value[..value.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    Some(match unit {
        'H' => Duration::from_secs(n * 3600),
        'M' => Duration::from_secs(n * 60),
        'S' => Duration::from_secs(n),
        'm' => Duration::from_millis(n),
        'u' => Duration::from_micros(n),
        'n' => Duration::from_nanos(n),
        _ => return None,
    })
}

pub fn budget_of<T>(request: &Request<T>) -> Option<Duration> {
    let value = request.metadata().get("grpc-timeout")?.to_str().ok()?;
    parse_grpc_timeout(value)
}

pub fn map_upstream(status: Status) -> Status {
    match status.code() {
        Code::NotFound => Status::invalid_argument("author: такого автора нет"),
        Code::Unavailable | Code::ResourceExhausted => Status::unavailable("сервис пользователей недоступен, повторите позже"),
        Code::DeadlineExceeded | Code::Cancelled => Status::deadline_exceeded("сервис пользователей не успел ответить"),
        _ => Status::internal("ошибка зависимого сервиса"),
    }
}

async fn fetch_author(users: &UsersClient<Channel>, id: u64, left: Duration, request_id: Option<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>) -> Result<User, Status> {
    if left.is_zero() {
        return Err(Status::deadline_exceeded("бюджет вызова закончился"));
    }
    let mut call = Request::new(GetUserRequest { id });
    call.set_timeout(left);
    if let Some(value) = request_id {
        call.metadata_mut().insert("x-request-id", value);
    }
    users.clone().get_user(call).await.map(|reply| reply.into_inner()).map_err(map_upstream)
}

struct FeedSvc {
    users: UsersClient<Channel>,
    posts: Arc<Mutex<Vec<StoredPost>>>,
}

#[tonic::async_trait]
impl Feed for FeedSvc {
    async fn publish(&self, request: Request<PublishRequest>) -> Result<Response<PublishReply>, Status> {
        let author = request.extensions().get::<UserId>().ok_or_else(|| Status::internal("перехватчик не положил UserId"))?.0;
        let text = request.into_inner().text.trim().to_string();
        if text.is_empty() || text.chars().count() > 280 {
            return Err(Status::invalid_argument("text: от 1 до 280 символов"));
        }
        let mut posts = self.posts.lock().unwrap();
        let id = posts.len() as u64 + 1;
        posts.push(StoredPost { id, author, text });
        Ok(Response::new(PublishReply { id }))
    }

    type TimelineStream = ReceiverStream<Result<Item, Status>>;

    async fn timeline(&self, request: Request<TimelineRequest>) -> Result<Response<Self::TimelineStream>, Status> {
        let started = Instant::now();
        let budget = budget_of(&request).unwrap_or(DEFAULT_BUDGET);
        let request_id = request.metadata().get("x-request-id").cloned();
        let limit = match request.get_ref().limit {
            0 => 20,
            n if n > 100 => return Err(Status::invalid_argument("limit: не больше 100")),
            n => n as usize,
        };
        let posts: Vec<StoredPost> = self.posts.lock().unwrap().iter().rev().take(limit).cloned().collect();
        let users = self.users.clone();
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut authors: HashMap<u64, User> = HashMap::new();
            for post in posts {
                let author = match authors.get(&post.author) {
                    Some(author) => author.clone(),
                    None => match fetch_author(&users, post.author, budget.saturating_sub(started.elapsed()), request_id.clone()).await {
                        Ok(author) => {
                            authors.insert(post.author, author.clone());
                            author
                        }
                        Err(status) => {
                            let _ = tx.send(Err(status)).await;
                            return;
                        }
                    },
                };
                if tx.send(Ok(Item { id: post.id, author: Some(author), text: post.text })).await.is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

pub async fn run(listener: TcpListener, users: Channel, config: Config, shutdown: oneshot::Receiver<()>) -> Result<(), tonic::transport::Error> {
    let svc = FeedSvc { users: UsersClient::new(users), posts: Arc::default() };
    let auth = Auth { tokens: Arc::new(config.tokens.iter().cloned().collect()) };
    let (reporter, health) = health_reporter();
    reporter.set_service_status(SERVICE, ServingStatus::Serving).await;
    let grace = config.grace;
    let signal = async move {
        let _ = shutdown.await;
        reporter.set_service_status(SERVICE, ServingStatus::NotServing).await;
        tokio::time::sleep(grace).await;
    };
    Server::builder()
        .layer(LogLayer::new(config.sink))
        .add_service(health)
        .add_service(FeedServer::with_interceptor(svc, auth))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), signal)
        .await
}
