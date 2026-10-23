mod avatars;
mod config;
mod error;
mod service;

use proto::MESSAGE_MAX;
use proto::social::v1::users_server::UsersServer;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

pub use avatars::{Avatar, Avatars};
pub use config::{Config, JOURNAL_ENV};
pub use error::{Error, Result};
pub use service::{LIST_LIMIT, UsersService};

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let core = AppConfig::default();
    let app = match &config.journal {
        Some(path) => App::restore(MemNetwork::new(), Notifiers::new(), core, path).await?,
        None => App::start(MemNetwork::new(), Notifiers::new(), core)?,
    };
    let users = UsersServer::new(UsersService::new(app.clone()))
        .max_decoding_message_size(MESSAGE_MAX)
        .max_encoding_message_size(MESSAGE_MAX);
    let served = Server::builder()
        .add_service(users)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            shutdown.cancelled().await
        })
        .await;
    app.close(config.drain).await;
    served.map_err(Error::Serve)
}
