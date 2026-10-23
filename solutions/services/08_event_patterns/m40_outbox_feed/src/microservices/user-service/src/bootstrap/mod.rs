use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use grpc_kit::{AccessLog, caller, unready_on_stop};
use journal::Journal;
use nats_engine::Bus;
use proto::MESSAGE_MAX;
use proto::social::v1::users_server::UsersServer;
use service_runtime::{Background, CancellationToken, Criticality, Health, Restart};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::client::Relay;
use crate::common::{Config, Error, NAME, Result};
use crate::controller::UsersController;
use crate::repository::{MemoryAvatars, MemoryUsers};
use crate::service::UserService;

type Served = UsersServer<UsersController>;

pub async fn connect(config: &Config) -> Result<Option<Bus>> {
    match &config.nats {
        Some(nats) => Bus::connect(nats.clone())
            .await
            .map(Some)
            .map_err(Error::Bus),
        None => Ok(None),
    }
}

const RELAY_RESTART: Restart = Restart {
    pause: Duration::from_secs(1),
    max_restarts: 10,
};

pub async fn build(config: &Config, health: &Health) -> Result<UsersController> {
    let mut background = Background::new(config.trail.clone());
    assemble(config, health, None, &mut background).await
}

async fn assemble(
    config: &Config,
    health: &Health,
    bus: Option<Bus>,
    background: &mut Background,
) -> Result<UsersController> {
    let (journal, entries) = Journal::open(config.journal.as_deref()).await?;
    let written = journal.health();
    health.register("journal", move || {
        let seen = written.check();
        async move { seen }
    });
    let users = Arc::new(MemoryUsers::restore(journal, entries)?);
    if let Some(bus) = bus {
        let relay = Relay::open(bus, config.topics.clone(), users.clone()).await;
        let relay = relay.map_err(Error::Bus)?;
        background.spawn(
            "релей outbox",
            Criticality::Optional,
            RELAY_RESTART,
            move || relay.clone().run(),
        );
    }
    let service = UserService::new(users, Arc::new(MemoryAvatars::default()));
    Ok(UsersController::new(Arc::new(service)))
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
    let bus = connect(&config).await?;
    let mut background = Background::new(config.trail.clone());
    let built = assemble(&config, lifecycle.health(), bus.clone(), &mut background).await?;
    let users = UsersServer::new(built)
        .max_decoding_message_size(MESSAGE_MAX)
        .max_encoding_message_size(MESSAGE_MAX);
    let admin = Admin::start(admin, &lifecycle);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    tokio::spawn(unready_on_stop::<Served>(
        reporter,
        lifecycle.health().clone(),
    ));
    let builder = match &config.tls {
        Some(tls) => Server::builder()
            .tls_config(tls.server())
            .map_err(Error::Tls)?,
        None => Server::builder(),
    };
    let closing = lifecycle.closing();
    let serving = builder
        .layer(AccessLog::new(NAME, config.log.clone()))
        .add_service(health)
        .add_service(InterceptedService::new(users, caller))
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
