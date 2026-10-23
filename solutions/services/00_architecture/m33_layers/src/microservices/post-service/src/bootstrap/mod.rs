use std::sync::Arc;

use grpc_kit::{AccessLog, Gate, HARD_STOP, bounded, channel, drain};
use journal::Journal;
use proto::social::v1::posts_server::PostsServer;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::common::{Config, Error, Result};
use crate::controller::PostsController;
use crate::repository::MemoryPosts;
use crate::service::PostService;

pub const GATEWAY: &str = "api-gateway";

type Served = PostsServer<PostsController>;

pub async fn build(config: &Config, stop: &CancellationToken) -> Result<PostsController> {
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let (journal, entries) = Journal::open(config.journal.as_deref()).await?;
    let posts = MemoryPosts::restore(journal, entries);
    let service = PostService::new(Arc::new(posts));
    Ok(PostsController::new(Arc::new(service), users, stop.clone()))
}

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let posts = PostsServer::new(build(&config, &shutdown).await?);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    let stopping = drain::<Served>(reporter, shutdown.clone(), config.grace);
    let (builder, gate) = match &config.tls {
        Some(tls) => (
            Server::builder()
                .tls_config(tls.server())
                .map_err(Error::Tls)?,
            Gate::only(&[GATEWAY]),
        ),
        None => (Server::builder(), Gate::open()),
    };
    let serving = builder
        .layer(AccessLog::new("post-service", config.log.clone()))
        .add_service(health)
        .add_service(InterceptedService::new(posts, gate))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), stopping);
    bounded(serving, shutdown, config.grace + HARD_STOP)
        .await
        .map_err(Error::Serve)
}
