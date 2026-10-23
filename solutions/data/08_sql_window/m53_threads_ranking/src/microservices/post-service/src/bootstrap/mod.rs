use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use grpc_kit::{AccessLog, Gate, Readiness, Upstream, channel, unready_on_stop};
use journal::Journal;
use nats_engine::Bus;
use proto::social::v1::posts_server::PostsServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Criticality, Lifecycle, Restart};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::client::{Previewer, Relay};
use crate::common::{Config, Error, LIKE_FLUSH, NAME, Result};
use crate::controller::PostsController;
use crate::repository::{
    DraftRepository, MIGRATIONS, MemoryPosts, OutboxRepository, PgPosts, PostQueries,
    PostRepository, ThreadQueries,
};

type Stores = (
    Arc<dyn PostRepository>,
    Arc<dyn DraftRepository>,
    Arc<dyn OutboxRepository>,
    Arc<dyn PostQueries>,
    Option<Arc<dyn ThreadQueries>>,
);
use crate::service::PostService;

pub const GATEWAY: &str = "api-gateway";

pub const AUTH: &str = "auth-service";

const RELAY_RESTART: Restart = Restart {
    pause: Duration::from_secs(1),
    max_restarts: 10,
};

type Served = PostsServer<PostsController>;

pub async fn connect(config: &Config) -> Result<Option<Bus>> {
    match &config.nats {
        Some(nats) => Bus::connect(nats.clone())
            .await
            .map(Some)
            .map_err(Error::Bus),
        None => Ok(None),
    }
}

pub async fn build(
    config: &Config,
    lifecycle: &Lifecycle,
    background: &mut Background,
    bus: Option<Bus>,
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
    let (posts, drafts, outbox, queries, threads): Stores = match &config.database {
        Some(database) => {
            let pool = db_engine::connect(database).await.map_err(Error::Db)?;
            db_engine::apply(&pool, &MIGRATIONS)
                .await
                .map_err(Error::Db)?;
            let posts = Arc::new(PgPosts::open(pool, config.node).await.map_err(Error::Db)?);
            let threads: Arc<dyn ThreadQueries> = posts.clone();
            (
                posts.clone(),
                posts.clone(),
                posts.clone(),
                posts,
                Some(threads),
            )
        }
        None => {
            let posts = Arc::new(MemoryPosts::restore(journal, entries).on_node(config.node));
            tokio::spawn(flush_likes(Arc::clone(&posts), lifecycle.closing()));
            (posts.clone(), posts.clone(), posts.clone(), posts, None)
        }
    };
    if let Some(bus) = bus {
        let relay = Relay::open(bus, config.topics.clone(), outbox).await;
        let relay = relay.map_err(Error::Bus)?;
        background.spawn(
            "релей outbox",
            Criticality::Optional,
            RELAY_RESTART,
            move || relay.clone().run(),
        );
    }
    let service = PostService::new(posts)
        .with_drafts(drafts)
        .with_queries(queries);
    let service = match threads {
        Some(threads) => service.with_threads(threads),
        None => service,
    };
    let controller = PostsController::new(Arc::new(service), users, lifecycle.closing());
    Ok(match config.previews {
        true => controller.with_previews(Arc::new(Previewer::new())),
        false => controller,
    })
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
    let bus = connect(&config).await?;
    let built = build(&config, &lifecycle, &mut background, bus.clone()).await?;
    let posts = PostsServer::new(built);
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
            Gate::only(&[GATEWAY, AUTH]),
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
    if let Some(bus) = bus {
        bus.close().await.map_err(Error::Bus)?;
    }
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}

async fn flush_likes(posts: Arc<MemoryPosts>, closing: CancellationToken) {
    let mut ticker = tokio::time::interval(LIKE_FLUSH);
    loop {
        tokio::select! {
            _ = closing.cancelled() => break,
            _ = ticker.tick() => {
                if let Err(error) = posts.flush().await {
                    tracing::warn!("{NAME}: лайки не сброшены в журнал: {error}");
                }
            }
        }
    }
    if let Err(error) = posts.flush().await {
        tracing::warn!("{NAME}: лайки при остановке не сброшены: {error}");
    }
}
