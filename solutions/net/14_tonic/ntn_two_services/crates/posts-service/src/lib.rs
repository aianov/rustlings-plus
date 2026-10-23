// Пост должен принадлежать существующему автору, а автора знает только сервис пользователей.
// Допиши создание и чтение постов и, главное, перевод ошибок users в то, что можно сказать
// своему клиенту (таблица в README).
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
    match status.code() {
        Code::NotFound => Status::invalid_argument("author_id: такого автора нет"),
        Code::Unavailable | Code::ResourceExhausted => Status::unavailable("сервис пользователей недоступен, повторите позже"),
        Code::DeadlineExceeded | Code::Cancelled => Status::deadline_exceeded("сервис пользователей не успел ответить"),
        _ => Status::internal("ошибка зависимого сервиса"),
    }
}

#[tonic::async_trait]
impl Posts for PostsSvc {
    async fn create_post(&self, request: Request<CreatePostRequest>) -> Result<Response<Post>, Status> {
        let request_id = request.metadata().get("x-request-id").cloned();
        let req = request.into_inner();
        let text = req.text.trim().to_string();
        if text.is_empty() {
            return Err(Status::invalid_argument("text: пост не может быть пустым"));
        }
        let mut call = Request::new(GetUserRequest { id: req.author_id });
        if let Some(value) = request_id {
            call.metadata_mut().insert("x-request-id", value);
        }
        let author = self.users.clone().get_user(call).await.map_err(map_upstream)?.into_inner();
        let mut state = self.state.lock().unwrap();
        state.last_id += 1;
        let post = Post { id: state.last_id, author: Some(author), text };
        state.posts.insert(post.id, post.clone());
        Ok(Response::new(post))
    }

    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let id = request.into_inner().id;
        let state = self.state.lock().unwrap();
        state.posts.get(&id).cloned().map(Response::new).ok_or_else(|| Status::not_found(format!("пост {id} не найден")))
    }
}

pub async fn run(listener: TcpListener, users: Channel) -> Result<(), tonic::transport::Error> {
    Server::builder().add_service(PostsServer::new(PostsSvc::new(users))).serve_with_incoming(TcpListenerStream::new(listener)).await
}
