use std::sync::Arc;

use grpc_kit::{AccessLog, HARD_STOP, bounded, caller, drain};
use journal::Journal;
use proto::MESSAGE_MAX;
use proto::social::v1::users_server::UsersServer;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::common::{Config, Error, Result};
use crate::controller::UsersController;
use crate::repository::{MemoryAvatars, MemoryUsers};
use crate::service::UserService;

type Served = UsersServer<UsersController>;

pub async fn build(config: &Config) -> Result<UsersController> {
    let (journal, entries) = Journal::open(config.journal.as_deref()).await?;
    let users = MemoryUsers::restore(journal, entries)?;
    let service = UserService::new(Arc::new(users), Arc::new(MemoryAvatars::default()));
    Ok(UsersController::new(Arc::new(service)))
}

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let users = UsersServer::new(build(&config).await?)
        .max_decoding_message_size(MESSAGE_MAX)
        .max_encoding_message_size(MESSAGE_MAX);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    let stopping = drain::<Served>(reporter, shutdown.clone(), config.grace);
    let builder = match &config.tls {
        Some(tls) => Server::builder()
            .tls_config(tls.server())
            .map_err(Error::Tls)?,
        None => Server::builder(),
    };
    let serving = builder
        .layer(AccessLog::new("user-service", config.log.clone()))
        .add_service(health)
        .add_service(InterceptedService::new(users, caller))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), stopping);
    bounded(serving, shutdown, config.grace + HARD_STOP)
        .await
        .map_err(Error::Serve)
}
