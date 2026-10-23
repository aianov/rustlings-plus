use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use grpc_kit::{AccessLog, Readiness, Upstream, caller, channel, unready_on_stop};
use journal::Journal;
use nats_engine::Bus;
use proto::social::v1::chat_server::ChatServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Criticality, Lifecycle, Restart};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::client::{BusTypist, Relay};
use crate::common::{Config, Error, NAME, Result};
use crate::controller::ChatController;
use crate::repository::MemoryChats;
use crate::service::{ChatService, Offline, Typist};

type Served = ChatServer<ChatController>;

const RELAY_RESTART: Restart = Restart {
    pause: Duration::from_secs(1),
    max_restarts: 10,
};

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
    bus: Option<Bus>,
    lifecycle: &Lifecycle,
    background: &mut Background,
) -> Result<ChatController> {
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let (journal, lines) = Journal::open(config.journal.as_deref()).await?;
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
    let chats = Arc::new(MemoryChats::restore(journal, lines));
    let typist: Arc<dyn Typist> = match bus {
        Some(bus) => {
            let relay = Relay::open(bus.clone(), config.topics.clone(), chats.clone()).await;
            let relay = relay.map_err(Error::Bus)?;
            background.spawn(
                "релей outbox",
                Criticality::Optional,
                RELAY_RESTART,
                move || relay.clone().run(),
            );
            Arc::new(BusTypist::new(bus, config.topics.clone()))
        }
        None => Arc::new(Offline),
    };
    let service = ChatService::new(chats, typist);
    Ok(ChatController::new(Arc::new(service), users))
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
    let controller = build(&config, bus.clone(), &lifecycle, &mut background).await?;
    let chat = ChatServer::new(controller);
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
        .add_service(InterceptedService::new(chat, caller))
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
