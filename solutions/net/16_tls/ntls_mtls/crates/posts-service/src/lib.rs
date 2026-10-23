// Сервис постов под mTLS: клиент обязан предъявить сертификат нашего CA, а вызывать методы может только шлюз.
// Допиши serve (настройка TLS, проверка личности).
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
    let read = |name: &str| std::fs::read_to_string(certs_dir.join(name));
    let tls = ServerTlsConfig::new().identity(Identity::from_pem(read("posts.pem")?, read("posts.key")?)).client_ca_root(Certificate::from_pem(read("ca.pem")?));
    let gateway_only = |request: Request<()>| -> Result<Request<()>, Status> {
        let certs = request.peer_certs().ok_or_else(|| Status::unauthenticated("нужен клиентский сертификат"))?;
        let first = certs.first().ok_or_else(|| Status::unauthenticated("пустая цепочка клиента"))?;
        if identity::dns_names(first.as_ref()).iter().any(|n| n == GATEWAY) {
            Ok(request)
        } else {
            Err(Status::permission_denied("сервис доступен только шлюзу"))
        }
    };
    Server::builder().tls_config(tls)?.add_service(PostsServer::with_interceptor(PostsSvc, gateway_only)).serve_with_incoming(TcpListenerStream::new(listener)).await?;
    Ok(())
}
