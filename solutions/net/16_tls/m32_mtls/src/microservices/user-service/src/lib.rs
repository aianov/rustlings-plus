mod avatars;
mod config;
mod error;
mod service;

use grpc_kit::{AccessLog, HARD_STOP, bounded, caller, drain};
use proto::MESSAGE_MAX;
use proto::social::v1::users_server::UsersServer;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

pub use avatars::{Avatar, Avatars};
pub use config::{Config, JOURNAL_ENV};
pub use error::{Error, Result};
pub use service::{LIST_LIMIT, UsersService};

type Served = UsersServer<UsersService>;

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let core = AppConfig::default();
    let app = match &config.journal {
        Some(path) => App::restore(MemNetwork::new(), Notifiers::new(), core, path).await?,
        None => App::start(MemNetwork::new(), Notifiers::new(), core)?,
    };
    let users = UsersServer::new(UsersService::new(app.clone()))
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
    let served = bounded(serving, shutdown, config.grace + HARD_STOP).await;
    app.close(config.drain).await;
    served.map_err(Error::Serve)
}
