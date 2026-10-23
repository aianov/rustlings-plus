// Сервис «Me» закрыт токеном, сервис «Public» открыт. Перехватчик сервера должен найти
// пользователя по токену до обработчика. Допиши Auth::call и сборку сервера.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::Interceptor;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("me.v1");
}
use pb::me_server::{Me, MeServer};
use pb::public_server::{Public, PublicServer};
use pb::{PingReply, PingRequest, WhoAmIReply, WhoAmIRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(pub u64);

#[derive(Clone)]
pub struct Auth {
    tokens: Arc<HashMap<String, u64>>,
}

impl Auth {
    pub fn new(tokens: &[(&str, u64)]) -> Self {
        Auth { tokens: Arc::new(tokens.iter().map(|(t, id)| (t.to_string(), *id)).collect()) }
    }
}

impl Interceptor for Auth {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let denied = || Status::unauthenticated("нужен действующий токен");
        let header = request.metadata().get("authorization").ok_or_else(denied)?;
        let header = header.to_str().map_err(|_| denied())?;
        let token = header.strip_prefix("Bearer ").ok_or_else(denied)?;
        let id = *self.tokens.get(token).ok_or_else(denied)?;
        request.extensions_mut().insert(UserId(id));
        Ok(request)
    }
}

struct MeSvc {
    calls: Arc<AtomicUsize>,
}

#[tonic::async_trait]
impl Me for MeSvc {
    async fn who_am_i(&self, request: Request<WhoAmIRequest>) -> Result<Response<WhoAmIReply>, Status> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let user = request.extensions().get::<UserId>().ok_or_else(|| Status::internal("перехватчик не положил UserId"))?;
        Ok(Response::new(WhoAmIReply { user_id: user.0 }))
    }
}

struct PublicSvc;

#[tonic::async_trait]
impl Public for PublicSvc {
    async fn ping(&self, _: Request<PingRequest>) -> Result<Response<PingReply>, Status> {
        Ok(Response::new(PingReply { text: "pong".to_string() }))
    }
}

pub async fn start(auth: Auth, calls: Arc<AtomicUsize>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let router = Server::builder().add_service(PublicServer::new(PublicSvc)).add_service(MeServer::with_interceptor(MeSvc { calls }, auth));
    tokio::spawn(async move {
        let _ = router.serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
