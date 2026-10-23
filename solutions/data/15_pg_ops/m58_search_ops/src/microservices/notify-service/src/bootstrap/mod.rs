use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use grpc_kit::channel;
use nats_engine::Bus;
use nats_engine::js::{StreamSpec, ensure_stream};
use service_runtime::{Background, CancellationToken};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::client::{BusCourier, GrpcAudience};
use crate::common::{Config, Error, Result, Stats, StatsSnapshot};
use crate::controller;
use crate::repository::{
    AHEAD_MONTHS, KEEP_MONTHS, MIGRATIONS, MemoryNotices, NoticeRepository, PgNotices,
};
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
    let stream = StreamSpec::new(&config.topics.stream(), config.topics.stream_subjects());
    ensure_stream(&bus, &stream).await.map_err(Error::Bus)?;
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
    let notices: Arc<dyn NoticeRepository> = match &config.database {
        Some(database) => {
            let pool = db_engine::prepare(database, &MIGRATIONS)
                .await
                .map_err(Error::Db)?;
            let notices = Arc::new(PgNotices::new(pool));
            tokio::spawn(maintain(Arc::clone(&notices), lifecycle.closing()));
            notices
        }
        None => Arc::new(MemoryNotices::new()),
    };
    let service = NotifyService::new(
        notices,
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

pub const MAINTAIN_EVERY: Duration = Duration::from_secs(60 * 60);

async fn maintain(notices: Arc<PgNotices>, closing: CancellationToken) {
    loop {
        if let Err(error) = notices.ensure_partitions(0, AHEAD_MONTHS).await {
            tracing::warn!("notify-service: партиции вперёд не созданы: {error}");
        }
        match notices.drop_older(KEEP_MONTHS).await {
            Ok(0) => {}
            Ok(dropped) => tracing::info!("notify-service: удалено старых партиций: {dropped}"),
            Err(error) => tracing::warn!("notify-service: старые партиции не удалены: {error}"),
        }
        tokio::select! {
            () = closing.cancelled() => return,
            () = tokio::time::sleep(MAINTAIN_EVERY) => {}
        }
    }
}
