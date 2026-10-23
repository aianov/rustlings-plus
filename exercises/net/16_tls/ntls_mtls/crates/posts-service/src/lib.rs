// Сервис постов под mTLS: клиент обязан предъявить сертификат нашего CA, а вызывать методы может только шлюз.
// Допиши serve (настройка TLS, проверка личности).
#![allow(unused_variables, dead_code, unused_imports)]

pub mod identity;

use proto::posts::v1::posts_server::{Posts, PostsServer};
use proto::posts::v1::{GetPostRequest, Post};
use std::path::Path;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};

pub const GATEWAY: &str = "gateway.social.test";

struct PostsSvc;

#[tonic::async_trait]
impl Posts for PostsSvc {
    async fn get_post(&self, request: Request<GetPostRequest>) -> Result<Response<Post>, Status> {
        let id = request.into_inner().id;
        Ok(Response::new(Post { id, text: format!("пост {id}") }))
    }
}

pub async fn serve(listener: TcpListener, certs_dir: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    todo!()
}
