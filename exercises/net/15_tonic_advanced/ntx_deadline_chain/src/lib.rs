// Сервис постов спрашивает автора у users и ставит этому вызову срок. Пока users быстрый, всё хорошо,
// но на медленном users получается беда: найди, что не так со сроком, и почини.
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};

pub mod users {
    pub mod v1 {
        tonic::include_proto!("users.v1");
    }
}
pub mod posts {
    pub mod v1 {
        tonic::include_proto!("posts.v1");
    }
}
use posts::v1::posts_server::{Posts, PostsServer};
use posts::v1::{GetPostRequest, Post};
use users::v1::users_client::UsersClient;
use users::v1::GetUserRequest;

pub const LOCAL_WORK: Duration = Duration::from_millis(150);
pub const DEFAULT_UPSTREAM: Duration = Duration::from_secs(1);

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

pub struct PostsSvc {
    users: UsersClient<Channel>,
}

#[tonic::async_trait]
impl Posts for PostsSvc {
    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let started = Instant::now();
        let budget = budget_of(&request);
        let id = request.into_inner().id;
        tokio::time::sleep(LOCAL_WORK).await;
        let mut call = Request::new(GetUserRequest { id });
        call.set_timeout(Duration::from_millis(300));
        let author = match self.users.clone().get_user(call).await {
            Ok(reply) => reply.into_inner(),
            Err(status) if matches!(status.code(), Code::Cancelled | Code::DeadlineExceeded) => return Err(Status::deadline_exceeded("users не успел ответить")),
            Err(_) => return Err(Status::unavailable("users недоступен")),
        };
        Ok(Response::new(Post { id, author: Some(author), text: format!("пост {id}") }))
    }
}

pub async fn start(users: Channel) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let svc = PostsSvc { users: UsersClient::new(users) };
    tokio::spawn(async move {
        let _ = Server::builder().add_service(PostsServer::new(svc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
