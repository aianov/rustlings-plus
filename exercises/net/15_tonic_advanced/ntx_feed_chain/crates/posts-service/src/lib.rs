// Лента через цепочку: токен, request-id, бюджет времени, стрим, здоровье и остановка.
// Заготовка даёт каркас; пробелы, помеченные todo!, заполняешь ты (список в README проекта).
#![allow(unused_variables, dead_code, unused_imports)]

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
    todo!()
}

async fn fetch_author(users: &UsersClient<Channel>, id: u64, left: Duration, request_id: Option<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>) -> Result<User, Status> {
    todo!()
}

struct FeedSvc {
    users: UsersClient<Channel>,
    posts: Arc<Mutex<Vec<StoredPost>>>,
}

#[tonic::async_trait]
impl Feed for FeedSvc {
    async fn publish(&self, request: Request<PublishRequest>) -> Result<Response<PublishReply>, Status> {
        todo!()
    }

    type TimelineStream = ReceiverStream<Result<Item, Status>>;

    async fn timeline(&self, request: Request<TimelineRequest>) -> Result<Response<Self::TimelineStream>, Status> {
        todo!()
    }
}

pub async fn run(listener: TcpListener, users: Channel, config: Config, shutdown: oneshot::Receiver<()>) -> Result<(), tonic::transport::Error> {
    todo!()
}
