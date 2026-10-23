use std::sync::Arc;

use admin::Admin;
use grpc_kit::{AccessLog, Readiness, Upstream, caller, channel, unready_on_stop};
use journal::Journal;
use proto::social::v1::chat_server::ChatServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Lifecycle};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Server;
use tonic_health::server::health_reporter;

use crate::common::{Config, Error, NAME, Result};
use crate::controller::ChatController;
use crate::repository::MemoryChats;
use crate::service::{ChatService, Hub};

type Served = ChatServer<ChatController>;

pub async fn build(
    config: &Config,
    hub: Arc<Hub>,
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
    let chats = MemoryChats::restore(journal, lines);
    let service = ChatService::new(Arc::new(chats), hub);
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
    let hub = Arc::new(Hub::default());
    let controller = build(&config, Arc::clone(&hub), &lifecycle, &mut background).await?;
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
            hub.close();
        });
    let halted = lifecycle.run(config.timing.wait, serving, background).await;
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}
