mod config;
mod error;
mod mirror;
mod service;

use proto::social::v1::posts_server::PostsServer;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Endpoint, Server};

pub use config::{Config, JOURNAL_ENV};
pub use error::{Error, Result};
pub use mirror::Mirror;
pub use service::PostsService;

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let users = Endpoint::from_shared(format!("http://{}", config.users))
        .map_err(Error::Upstream)?
        .connect_lazy();
    let core = AppConfig::default();
    let app = match &config.journal {
        Some(path) => App::restore(MemNetwork::new(), Notifiers::new(), core, path).await?,
        None => App::start(MemNetwork::new(), Notifiers::new(), core)?,
    };
    let posts = PostsServer::new(PostsService::new(app.clone(), Mirror::new(users)));
    let served = Server::builder()
        .add_service(posts)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            shutdown.cancelled().await
        })
        .await;
    app.close(config.drain).await;
    served.map_err(Error::Serve)
}
