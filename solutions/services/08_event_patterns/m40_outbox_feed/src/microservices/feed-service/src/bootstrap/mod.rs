use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use grpc_kit::{AccessLog, Gate, unready_on_stop};
use nats_engine::Bus;
use nats_engine::js::{StreamSpec, ensure_stream};
use proto::MESSAGE_MAX;
use proto::social::v1::feeds_server::FeedsServer;
use service_runtime::{Background, CancellationToken, Criticality, Restart};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::common::{Config, Error, NAME, Result};
use crate::controller::{FeedsController, Progress, follow};
use crate::repository::MemoryFeed;
use crate::service::FeedService;

pub const GATEWAY: &str = "api-gateway";

const FOLLOW_RESTART: Restart = Restart {
    pause: Duration::from_millis(500),
    max_restarts: 20,
};

type Served = FeedsServer<FeedsController>;

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
    let bus = Bus::connect(config.nats.clone())
        .await
        .map_err(Error::Bus)?;
    let stream = StreamSpec::new(&config.topics.stream(), config.topics.stream_subjects());
    ensure_stream(&bus, &stream).await.map_err(Error::Bus)?;
    let service = Arc::new(FeedService::new(Arc::new(MemoryFeed::new())));
    let progress = Arc::new(Progress::default());
    let watched = Arc::clone(&progress);
    lifecycle.health().register("projection", move || {
        let (caught, applied) = (watched.caught_up(), watched.applied());
        async move {
            match caught {
                true => Ok(()),
                false => Err(format!("догоняю stream событий, применено до {applied}")),
            }
        }
    });
    let mut background = Background::new(config.trail.clone());
    let (follower, topics, consumer) = (bus.clone(), config.topics.clone(), Arc::clone(&service));
    background.spawn(
        "проекция ленты",
        Criticality::Optional,
        FOLLOW_RESTART,
        move || {
            let (bus, topics, service, progress) = (
                follower.clone(),
                topics.clone(),
                Arc::clone(&consumer),
                Arc::clone(&progress),
            );
            async move {
                if let Err(error) = follow(bus, topics, service, progress).await {
                    eprintln!("{NAME}: чтение stream прервалось: {error}");
                }
            }
        },
    );
    let feeds = FeedsServer::new(FeedsController::new(service))
        .max_decoding_message_size(MESSAGE_MAX)
        .max_encoding_message_size(MESSAGE_MAX);
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
        .add_service(InterceptedService::new(feeds, gate))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            closing.cancelled().await;
        });
    let halted = lifecycle.run(config.timing.wait, serving, background).await;
    bus.close().await.map_err(Error::Bus)?;
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}
