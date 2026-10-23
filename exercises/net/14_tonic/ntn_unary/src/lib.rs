// Схема и build.rs готовы. Сервис пользователей хранит всё в памяти: нужно научить его отвечать
// и запускаться на свободном порту так, чтобы тест сразу знал адрес. Допиши обработчики и start.
#![allow(unused_variables)]

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
        todo!()
    }

    async fn create_user(&self, request: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        todo!()
    }
}

pub async fn start(svc: UsersSvc) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
    todo!()
}
