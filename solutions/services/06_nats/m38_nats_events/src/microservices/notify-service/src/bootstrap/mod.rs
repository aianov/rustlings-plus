use std::sync::Arc;

use admin::Admin;
use grpc_kit::channel;
use nats_engine::Bus;
use service_runtime::{Background, CancellationToken};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::client::{BusCourier, GrpcAudience};
use crate::common::{Config, Error, Result, Stats, StatsSnapshot};
use crate::controller;
use crate::repository::MemoryNotices;
use crate::service::NotifyService;

pub struct Running {
    stats: Arc<Stats>,
    bus: Bus,
    done: JoinHandle<Result<()>>,
}

impl Running {
    pub fn stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    pub fn ready(&self) -> bool {
        self.bus.ready()
    }

    pub async fn finished(self) -> Result<()> {
        match self.done.await {
            Ok(done) => done,
            Err(error) => Err(Error::Io(std::io::Error::other(error))),
        }
    }
}

pub async fn run(config: Config, shutdown: CancellationToken) -> Result<()> {
    start(None, config, shutdown).await?.finished().await
}

pub async fn serve(admin: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    start(Some(admin), config, shutdown).await?.finished().await
}

pub async fn start(
    admin: Option<TcpListener>,
    config: Config,
    shutdown: CancellationToken,
) -> Result<Running> {
    let lifecycle = config.lifecycle(&shutdown);
    let bus = Bus::connect(config.nats.clone())
        .await
        .map_err(Error::Bus)?;
    let watched = bus.clone();
    lifecycle.health().register("nats", move || {
        let up = watched.ready();
        async move {
            match up {
                true => Ok(()),
                false => Err(String::from("нет связи с NATS")),
            }
        }
    });
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let service = NotifyService::new(
        Arc::new(MemoryNotices::new()),
        Arc::new(GrpcAudience::new(users, config.audience_timeout)),
        Arc::new(BusCourier::new(bus.clone(), config.topics.clone())),
    );
    let stats = Arc::new(Stats::default());
    let controller = controller::start(&bus, &config, Arc::new(service), Arc::clone(&stats))
        .await
        .map_err(Error::Bus)?;
    let admin = Admin::start(admin, &lifecycle);
    let closing = lifecycle.closing();
    let flushed = bus.clone();
    let serving = async move {
        closing.cancelled().await;
        controller.stop().await;
        flushed.close().await
    };
    let background = Background::new(config.trail.clone());
    let wait = config.timing.wait;
    let done = tokio::spawn(async move {
        let halted = lifecycle.run(wait, serving, background).await;
        admin.stop().await.map_err(Error::Io)?;
        halted.map_err(Error::from)
    });
    Ok(Running { stats, bus, done })
}
