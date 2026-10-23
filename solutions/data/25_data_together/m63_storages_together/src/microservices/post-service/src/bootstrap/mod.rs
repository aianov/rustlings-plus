use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use grpc_kit::{AccessLog, Gate, Readiness, Upstream, channel, unready_on_stop};
use nats_engine::Bus;
use proto::social::v1::posts_server::PostsServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Criticality, Lifecycle, Restart};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::client::{
    LIKE_FLUSH_EVERY, Previewer, RedisHeat, RedisLikes, Relay, flush_pending_likes,
};
use resp_client::Client;

use crate::common::{Config, Error, LIKE_FLUSH, NAME, Result};
use crate::controller::PostsController;
use crate::repository::{Extras, MIGRATIONS, PgPosts, PostRepository, Stores};

use crate::service::{PostService, TRENDS_REFRESH, refresh_trends};

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
    let upstream = Upstream {
        label: "user-service",
        service: users_server::SERVICE_NAME,
        channel: users.clone(),
    };
    Readiness::supervised(vec![upstream], background).register(lifecycle.health());
    let stores = match (&config.database, &config.stores) {
        (Some(database), _) => {
            let pool = db_engine::prepare(database, &MIGRATIONS)
                .await
                .map_err(Error::Db)?;
            let posts = PgPosts::open(pool, config.node).await.map_err(Error::Db)?;
            let posts = match &config.redis {
                Some(redis) => {
                    let instance = format!("posts-{}-{}", config.node, std::process::id());
                    let likes =
                        Arc::new(RedisLikes::new(Client::connect(redis.clone()), &instance));
                    let posts = Arc::new(posts.with_pending(likes.clone()));
                    tokio::spawn(flush_pending_likes(
                        likes,
                        Arc::clone(&posts),
                        LIKE_FLUSH_EVERY,
                        lifecycle.closing(),
                    ));
                    posts
                }
                None => Arc::new(posts),
            };
            tokio::spawn(reconcile_likes(
                Arc::clone(&posts),
                RECONCILE_EVERY,
                lifecycle.closing(),
            ));
            Stores::postgres(posts)
        }
        (None, Some(stores)) => stores.clone(),
        (None, None) => return Err(Error::NoStorage),
    };
    if let Some(extras) = &stores.extras {
        tokio::spawn(refresh_trends(
            Arc::clone(&extras.fulltext),
            TRENDS_REFRESH,
            lifecycle.closing(),
        ));
    }
    tokio::spawn(flush_likes(Arc::clone(&stores.posts), lifecycle.closing()));
    let (posts, drafts, outbox, queries) = (
        stores.posts.clone(),
        stores.drafts.clone(),
        stores.outbox.clone(),
        stores.queries.clone(),
    );
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
    let service = match &config.redis {
        Some(redis) => service.with_heat(Arc::new(RedisHeat::new(Client::connect(redis.clone())))),
        None => service,
    };
    let service = match stores.extras {
        Some(Extras {
            threads,
            pins,
            search,
            fulltext,
        }) => service
            .with_fulltext(fulltext)
            .with_threads(threads)
            .with_pins(pins)
            .with_search(search),
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

async fn flush_likes(posts: Arc<dyn PostRepository>, closing: CancellationToken) {
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

pub const RECONCILE_EVERY: Duration = Duration::from_secs(15 * 60);

async fn reconcile_likes(posts: Arc<PgPosts>, every: Duration, closing: CancellationToken) {
    loop {
        tokio::select! {
            () = closing.cancelled() => return,
            () = tokio::time::sleep(every) => {}
        }
        match posts.reconcile_likes().await {
            Ok(0) => {}
            Ok(fixed) => tracing::warn!("{NAME}: сверка поправила счётчик лайков у {fixed} постов"),
            Err(error) => tracing::warn!("{NAME}: сверка лайков не прошла: {error}"),
        }
    }
}
