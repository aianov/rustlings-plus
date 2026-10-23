// Схема и build.rs готовы. Сервис пользователей хранит всё в памяти: нужно научить его отвечать
// и запускаться на свободном порту так, чтобы тест сразу знал адрес. Допиши обработчики и start.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
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

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let id = request.into_inner().id;
        let state = self.state.lock().unwrap();
        match state.users.get(&id) {
            Some(user) => Ok(Response::new(user.clone())),
            None => Err(Status::not_found(format!("пользователь {id} не найден"))),
        }
    }

    async fn create_user(&self, request: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        let req = request.into_inner();
        let mut state = self.state.lock().unwrap();
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
