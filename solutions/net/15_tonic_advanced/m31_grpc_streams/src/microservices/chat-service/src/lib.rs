mod config;
mod error;
mod hub;
mod service;
mod store;

use std::sync::Arc;

use grpc_kit::{AccessLog, HARD_STOP, bounded, caller, drain};
use proto::social::v1::chat_server::ChatServer;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{Endpoint, Server};
use tonic_health::server::health_reporter;

pub use config::{Config, JOURNAL_ENV};
pub use error::{Error, Result};
pub use hub::{BACKLOG, Hub};
pub use service::{ChatService, HISTORY_DEFAULT, HISTORY_MAX};
pub use store::{Line, Refused, Store};

type Served = ChatServer<ChatService>;

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let users = Endpoint::from_shared(format!("http://{}", config.users))
        .map_err(Error::Upstream)?
        .connect_lazy();
    let store = Arc::new(Store::open(config.journal.as_deref()).await?);
    let hub = Arc::new(Hub::default());
    let chat = ChatServer::new(ChatService::new(store, Arc::clone(&hub), users));
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    let stopping = drain::<Served>(reporter, shutdown.clone(), config.grace);
    let closing = shutdown.clone();
    tokio::spawn(async move {
        closing.cancelled().await;
        hub.close();
    });
    let serving = Server::builder()
        .layer(AccessLog::new("chat-service", config.log.clone()))
        .add_service(health)
        .add_service(InterceptedService::new(chat, caller))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), stopping);
    bounded(serving, shutdown, config.grace + HARD_STOP)
        .await
        .map_err(Error::Serve)
}
