mod config;
mod error;
mod mirror;
mod service;

use grpc_kit::{AccessLog, HARD_STOP, bounded, caller, drain};
use proto::social::v1::posts_server::PostsServer;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{Endpoint, Server};
use tonic_health::server::health_reporter;

pub use config::{Config, JOURNAL_ENV};
pub use error::{Error, Result};
pub use mirror::Mirror;
pub use service::{PostsService, RECENT_MAX, SYNC_BUDGET};

type Served = PostsServer<PostsService>;

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let users = Endpoint::from_shared(format!("http://{}", config.users))
        .map_err(Error::Upstream)?
        .connect_lazy();
    let core = AppConfig::default();
    let app = match &config.journal {
        Some(path) => App::restore(MemNetwork::new(), Notifiers::new(), core, path).await?,
        None => App::start(MemNetwork::new(), Notifiers::new(), core)?,
    };
    let service = PostsService::new(app.clone(), Mirror::new(users), shutdown.clone());
    let posts = PostsServer::new(service);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    let stopping = drain::<Served>(reporter, shutdown.clone(), config.grace);
    let serving = Server::builder()
        .layer(AccessLog::new("post-service", config.log.clone()))
        .add_service(health)
        .add_service(InterceptedService::new(posts, caller))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), stopping);
    let served = bounded(serving, shutdown, config.grace + HARD_STOP).await;
    app.close(config.drain).await;
    served.map_err(Error::Serve)
}
