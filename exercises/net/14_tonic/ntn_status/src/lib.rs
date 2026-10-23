// Сервис пользователей должен отвечать осмысленными кодами, а клиент должен их разбирать:
// «пользователя нет» и «сервис упал» это разные новости. Допиши сервер и два помощника клиента.
#![allow(unused_variables)]

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
        todo!()
    }

    async fn create_user(&self, request: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        todo!()
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
    todo!()
}

pub async fn register(client: &mut UsersClient<Channel>, handle: &str) -> Result<Registration, Status> {
    todo!()
}
