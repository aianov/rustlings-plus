// gRPC по TLS: сервер уже настроен, а клиент написан криво и либо не подключается, либо проверяет
// не то, что нужно. Найди оба дефекта в client_channel.
#![allow(unused_variables)]
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("users.v1");
}
use pb::users_server::{Users, UsersServer};
use pb::{GetUserRequest, User};

struct UsersSvc;

#[tonic::async_trait]
impl Users for UsersSvc {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let id = request.into_inner().id;
        Ok(Response::new(User { id, handle: format!("user{id}") }))
    }
}

pub async fn serve_tls(listener: TcpListener, cert_pem: &str, key_pem: &str) -> Result<(), tonic::transport::Error> {
    let tls = ServerTlsConfig::new().identity(Identity::from_pem(cert_pem, key_pem));
    Server::builder().tls_config(tls)?.add_service(UsersServer::new(UsersSvc)).serve_with_incoming(TcpListenerStream::new(listener)).await
}

pub fn client_channel(addr: SocketAddr, ca_pem: &str, domain: &str) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    let tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(ca_pem));
    Ok(Endpoint::from_shared(format!("http://{addr}"))?.tls_config(tls)?.connect_lazy())
}
