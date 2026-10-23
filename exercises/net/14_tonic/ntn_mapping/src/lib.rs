// На границе сервиса protobuf превращается в доменные типы: ноль вместо номера автора, пустой
// текст и неизвестное значение перечисления должны отсеиваться здесь, с понятной ошибкой.
// Допиши два преобразования.
#![allow(unused_variables)]

use std::net::SocketAddr;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("posts.v1");
}
use pb::posts_server::{Posts, PostsServer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorId(pub NonZeroU64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Followers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPost {
    pub author: AuthorId,
    pub text: String,
    pub visibility: Visibility,
}

impl TryFrom<pb::CreatePostRequest> for NewPost {
    type Error = Status;

    fn try_from(req: pb::CreatePostRequest) -> Result<Self, Status> {
        todo!()
    }
}

pub fn post_to_proto(id: u64, post: &NewPost) -> pb::Post {
    todo!()
}

#[derive(Default)]
pub struct PostsSvc {
    last_id: AtomicU64,
}

#[tonic::async_trait]
impl Posts for PostsSvc {
    async fn create_post(&self, request: Request<pb::CreatePostRequest>) -> Result<Response<pb::Post>, Status> {
        let new = NewPost::try_from(request.into_inner())?;
        let id = self.last_id.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Response::new(post_to_proto(id, &new)))
    }
}

pub async fn start() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(PostsServer::new(PostsSvc::default())).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
