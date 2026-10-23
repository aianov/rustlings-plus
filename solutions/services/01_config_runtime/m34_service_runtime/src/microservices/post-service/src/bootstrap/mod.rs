use std::sync::Arc;

use admin::Admin;
use grpc_kit::{AccessLog, Gate, Readiness, Upstream, channel, unready_on_stop};
use journal::Journal;
use proto::social::v1::posts_server::PostsServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Lifecycle};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::common::{Config, Error, NAME, Result};
use crate::controller::PostsController;
use crate::repository::MemoryPosts;
use crate::service::PostService;

pub const GATEWAY: &str = "api-gateway";

type Served = PostsServer<PostsController>;

pub async fn build(
    config: &Config,
    lifecycle: &Lifecycle,
    background: &mut Background,
) -> Result<PostsController> {
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let (journal, entries) = Journal::open(config.journal.as_deref()).await?;
    let written = journal.health();
    lifecycle.health().register("journal", move || {
        let seen = written.check();
        async move { seen }
    });
    let upstream = Upstream {
        label: "user-service",
        service: users_server::SERVICE_NAME,
        channel: users.clone(),
    };
    Readiness::supervised(vec![upstream], background).register(lifecycle.health());
    let posts = MemoryPosts::restore(journal, entries);
    let service = PostService::new(Arc::new(posts));
    Ok(PostsController::new(
        Arc::new(service),
        users,
        lifecycle.closing(),
    ))
}

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    launch(listener, None, config, shutdown).await
}

pub async fn serve(
    listener: TcpListener,
    admin: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(listener, Some(admin), config, shutdown).await
}

async fn launch(
    listener: TcpListener,
    admin: Option<TcpListener>,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    let lifecycle = config.lifecycle(&shutdown);
    let mut background = Background::new(config.trail.clone());
    let posts = PostsServer::new(build(&config, &lifecycle, &mut background).await?);
    let admin = Admin::start(admin, &lifecycle);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    tokio::spawn(unready_on_stop::<Served>(
        reporter,
        lifecycle.health().clone(),
    ));
    let (builder, gate) = match &config.tls {
        Some(tls) => (
            Server::builder()
                .tls_config(tls.server())
                .map_err(Error::Tls)?,
            Gate::only(&[GATEWAY]),
        ),
        None => (Server::builder(), Gate::open()),
    };
    let closing = lifecycle.closing();
    let serving = builder
        .layer(AccessLog::new(NAME, config.log.clone()))
        .add_service(health)
        .add_service(InterceptedService::new(posts, gate))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            closing.cancelled().await;
        });
    let halted = lifecycle.run(config.timing.wait, serving, background).await;
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}
