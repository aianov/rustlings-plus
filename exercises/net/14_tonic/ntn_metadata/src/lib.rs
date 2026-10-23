// Сквозной x-request-id и бинарная трасса: сервер возвращает их в ответе, клиент кладёт в запрос.
// Заголовки приходят из сети, поэтому доверять им нельзя ни в одну сторону.
#![allow(unused_variables)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_client::UsersClient;
use pb::users_server::{Users, UsersServer};
use pb::{CreateUserRequest, GetUserRequest, User};

#[derive(Default)]
pub struct UsersSvc {
    generated: AtomicU64,
}

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        todo!()
    }

    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("здесь не нужен"))
    }
}

pub async fn start() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(UsersServer::new(UsersSvc::default())).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}

#[derive(Debug, PartialEq)]
pub struct Reply {
    pub user: User,
    pub request_id: String,
    pub trace: Option<Vec<u8>>,
}

pub async fn call_with(client: &mut UsersClient<Channel>, id: u64, request_id: &str, trace: Option<&[u8]>) -> Result<Reply, Status> {
    todo!()
}
