// Сервис пользователей должен отвечать осмысленными кодами, а клиент должен их разбирать:
// «пользователя нет» и «сервис упал» это разные новости. Допиши сервер и два помощника клиента.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::users_server::{Users, UsersServer};
use pb::{CreateUserRequest, GetUserRequest, User};

#[derive(Default)]
struct State {
    last_id: u64,
    users: HashMap<u64, User>,
}

#[derive(Default)]
pub struct UsersSvc {
    state: Mutex<State>,
}

impl UsersSvc {
    pub fn new() -> Self {
        Self::default()
    }
}

fn valid_handle(handle: &str) -> bool {
    (3..=20).contains(&handle.chars().count()) && handle.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let id = request.into_inner().id;
        let state = self.state.lock().unwrap();
        state.users.get(&id).cloned().map(Response::new).ok_or_else(|| Status::not_found(format!("пользователь {id} не найден")))
    }

    async fn create_user(&self, request: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        let req = request.into_inner();
        if !valid_handle(&req.handle) {
            return Err(Status::invalid_argument("handle: от 3 до 20 символов из A-Z a-z 0-9 _"));
        }
        let mut state = self.state.lock().unwrap();
        if state.users.values().any(|u| u.handle.eq_ignore_ascii_case(&req.handle)) {
            return Err(Status::already_exists(format!("ник {} занят", req.handle)));
        }
        state.last_id += 1;
        let user = User { id: state.last_id, handle: req.handle, display_name: req.display_name };
        state.users.insert(user.id, user.clone());
        Ok(Response::new(user))
    }
}

pub async fn start(svc: UsersSvc) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let handle = tokio::spawn(async move {
        let _ = Server::builder().add_service(UsersServer::new(svc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok((addr, handle))
}

#[derive(Debug, PartialEq)]
pub enum Registration {
    Created(User),
    HandleTaken,
}

pub async fn find_user(client: &mut UsersClient<Channel>, id: u64) -> Result<Option<User>, Status> {
    match client.get_user(GetUserRequest { id }).await {
        Ok(reply) => Ok(Some(reply.into_inner())),
        Err(status) if status.code() == Code::NotFound => Ok(None),
        Err(status) => Err(status),
    }
}

pub async fn register(client: &mut UsersClient<Channel>, handle: &str) -> Result<Registration, Status> {
    match client.create_user(CreateUserRequest { handle: handle.to_string(), display_name: String::new() }).await {
        Ok(reply) => Ok(Registration::Created(reply.into_inner())),
        Err(status) if status.code() == Code::AlreadyExists => Ok(Registration::HandleTaken),
        Err(status) => Err(status),
    }
}
