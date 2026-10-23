// Когда база падает, текст её ошибки уезжает клиенту вместе с адресом и паролем. Тесты на
// правильные ответы проходят, поэтому проблема видна только в сообщении при поломке.
// Исправь превращение доменных ошибок в Status.
use std::fmt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_server::{Users, UsersServer};
use pb::{CreateUserRequest, GetUserRequest, User};

#[derive(Debug)]
pub enum DomainError {
    NotFound(u64),
    HandleTaken(String),
    InvalidHandle(String),
    Db(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::NotFound(id) => write!(f, "пользователь {id} не найден"),
            DomainError::HandleTaken(h) => write!(f, "ник {h} занят"),
            DomainError::InvalidHandle(h) => write!(f, "ник {h:?} не подходит"),
            DomainError::Db(e) => write!(f, "ошибка базы: {e}"),
        }
    }
}

impl From<DomainError> for Status {
    fn from(e: DomainError) -> Status {
        Status::internal(e.to_string())
    }
}

pub type Repo = Box<dyn Fn(Call) -> Result<User, DomainError> + Send + Sync>;

pub enum Call {
    Get(u64),
    Create(String),
}

pub type Journal = Arc<Mutex<Vec<String>>>;

pub struct UsersSvc {
    repo: Repo,
    journal: Journal,
}

impl UsersSvc {
    pub fn new(repo: Repo, journal: Journal) -> Self {
        UsersSvc { repo, journal }
    }

    fn run(&self, call: Call) -> Result<Response<User>, Status> {
        (self.repo)(call).map(Response::new).map_err(|e| {
            self.journal.lock().unwrap().push(e.to_string());
            Status::from(e)
        })
    }
}

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        self.run(Call::Get(request.into_inner().id))
    }

    async fn create_user(&self, request: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        self.run(Call::Create(request.into_inner().handle))
    }
}

pub async fn start(svc: UsersSvc) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(UsersServer::new(svc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
