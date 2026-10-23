use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use files::FsFileStore;
use grpc_kit::{AccessLog, caller, unready_on_stop};
use jobs::{Leader, Leases, now_ms};
use journal::Journal;
use nats_engine::{Bus, fresh_id};
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
use crate::service::{ORPHAN_GRACE, UserService};

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
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()))
        .with_lifecycle(users.clone())
        .with_files(Arc::new(FsFileStore::new(&config.files)));
    let service = Arc::new(service);
    if let Some(bus) = bus {
        let leases = format!("{}_ORPHANS", config.topics.leases());
        let every = config.orphan_sweep;
        let leases = Leases::open(&bus, &leases, every * 2)
            .await
            .map_err(Error::Bus)?;
        let leader = Leader::new("orphans", &fresh_id(NAME), every, leases, now_ms());
        tokio::spawn(orphans(Arc::clone(&service), leader));
        let relay = Relay::open(bus, config.topics.clone(), users.clone()).await;
        let relay = relay.map_err(Error::Bus)?;
        background.spawn(
            "релей outbox",
            Criticality::Optional,
            RELAY_RESTART,
            move || relay.clone().run(),
        );
    }
    Ok(UsersController::new(service))
}

async fn orphans(service: Arc<UserService>, mut leader: Leader) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        let service = Arc::clone(&service);
        let swept = leader
            .tick(now_ms(), |_| async move {
                let removed = service.sweep(now_ms() / 1000, ORPHAN_GRACE).await;
                if removed > 0 {
                    eprintln!("{NAME}: убрано сирот: {removed}");
                }
            })
            .await;
        if swept.is_err() {
            ticker.tick().await;
        }
    }
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
