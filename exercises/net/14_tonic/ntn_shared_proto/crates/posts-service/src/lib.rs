use proto::posts::v1::posts_server::{Posts, PostsServer};
use proto::posts::v1::{GetPostRequest, Post};
use proto::users::v1::User;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub struct PostsSvc;

#[tonic::async_trait]
impl Posts for PostsSvc {
    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let id = request.into_inner().id;
        let author = User { id: 1, handle: "ann".to_string(), display_name: "Ann".to_string() };
        Ok(Response::new(Post { id, author: Some(author), text: format!("пост {id}") }))
    }
}

pub async fn start() -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(PostsServer::new(PostsSvc)).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
