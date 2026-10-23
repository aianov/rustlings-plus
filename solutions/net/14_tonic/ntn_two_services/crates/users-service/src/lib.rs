use proto::users::v1::users_server::{Users, UsersServer};
use proto::users::v1::{CreateUserRequest, GetUserRequest, User};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub struct UsersSvc;

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        match request.into_inner().id {
            1 => Ok(Response::new(User { id: 1, handle: "ann".to_string(), display_name: "Ann".to_string() })),
            2 => Ok(Response::new(User { id: 2, handle: "bob".to_string(), display_name: "Bob".to_string() })),
            id => Err(Status::not_found(format!("пользователь {id} не найден"))),
        }
    }

    async fn create_user(&self, _: Request<CreateUserRequest>) -> Result<Response<User>, Status> {
        Err(Status::unimplemented("не нужен"))
    }
}

pub async fn start() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(UsersServer::new(UsersSvc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
