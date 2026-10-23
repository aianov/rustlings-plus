// Пост должен принадлежать существующему автору, а автора знает только сервис пользователей.
// Допиши создание и чтение постов и, главное, перевод ошибок users в то, что можно сказать
// своему клиенту (таблица в README).
#![allow(unused_variables, dead_code)]

use proto::posts::v1::posts_server::{Posts, PostsServer};
use proto::posts::v1::{CreatePostRequest, GetPostRequest, Post};
use proto::users::v1::users_client::UsersClient;
use proto::users::v1::GetUserRequest;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Response, Status};

#[derive(Default)]
struct State {
    last_id: u64,
    posts: HashMap<u64, Post>,
}

pub struct PostsSvc {
    users: UsersClient<Channel>,
    state: Mutex<State>,
}

impl PostsSvc {
    pub fn new(users: Channel) -> Self {
        PostsSvc { users: UsersClient::new(users), state: Mutex::default() }
    }
}

pub fn map_upstream(status: Status) -> Status {
    todo!()
}

#[tonic::async_trait]
impl Posts for PostsSvc {
    async fn create_post(&self, request: Request<CreatePostRequest>) -> Result<Response<Post>, Status> {
        todo!()
    }

    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        todo!()
    }
}

pub async fn run(listener: TcpListener, users: Channel) -> Result<(), tonic::transport::Error> {
    Server::builder().add_service(PostsServer::new(PostsSvc::new(users))).serve_with_incoming(TcpListenerStream::new(listener)).await
}
