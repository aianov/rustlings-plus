mod state;

use std::sync::Arc;
use std::time::Duration;

use admin::Admin;
use contracts::SystemClock;
use grpc_kit::{AccessLog, Readiness, Upstream, channel, unready_on_stop};
use journal::Journal;
use proto::social::v1::auth_server::AuthServer;
use proto::social::v1::users_server;
use service_runtime::{Background, CancellationToken, Criticality, Lifecycle, Restart};
use tls_edge::{mutual_server_config, terminate};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic_health::server::health_reporter;

use crate::client::LogMailer;
use crate::common::{Config, Error, NAME, Result};
use crate::controller::{AuthController, app};
use crate::repository::{MemoryAccounts, MemoryOneTime, MemorySessions};
use crate::service::{AuthParts, AuthService, KeyRing, Passwords};

pub use state::Shared;

type Served = AuthServer<AuthController>;

pub const SWEEP_EVERY: Duration = Duration::from_secs(60);

const SWEEP_RESTART: Restart = Restart {
    pause: Duration::from_secs(1),
    max_restarts: 3,
};

pub async fn build(
    config: Config,
    lifecycle: &Lifecycle,
    background: &mut Background,
) -> Result<Arc<Shared>> {
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let (journal, accounts) = Journal::open(config.journal.as_deref()).await?;
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
    let accounts = MemoryAccounts::restore(journal, accounts);
    let passwords = Passwords::new(config.cost, config.hash_parallel).map_err(Error::Password)?;
    let keys = match config.keys.clone() {
        Some(keys) => keys,
        None => {
            config.trail.note(
                "ключ подписи сгенерирован при старте: после перезапуска выданные токены не примутся, задай AUTH_SIGNING_KEYS",
            );
            KeyRing::random().ok_or(Error::Random)?
        }
    };
    let service = Arc::new(AuthService::new(AuthParts {
        accounts: Arc::new(accounts),
        sessions: Arc::new(MemorySessions::new(config.lifetimes.leeway)),
        once: Arc::new(MemoryOneTime::default()),
        passwords: Arc::new(passwords),
        keys,
        clock: Arc::new(SystemClock),
        mailer: Arc::new(LogMailer::new(config.trail.clone())),
        lifetimes: config.lifetimes,
        moderators: config.moderators.clone(),
        public_url: config.public_url.clone(),
    }));
    let sweeper = Arc::clone(&service);
    background.spawn(
        "чистка сессий",
        Criticality::Optional,
        SWEEP_RESTART,
        move || {
            let service = Arc::clone(&sweeper);
            async move {
                loop {
                    tokio::time::sleep(SWEEP_EVERY).await;
                    service.purge().await;
                }
            }
        },
    );
    Ok(Arc::new(Shared {
        service,
        users,
        config,
    }))
}

pub async fn run(
    http: TcpListener,
    grpc: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(http, grpc, None, config, shutdown).await
}

pub async fn serve(
    http: TcpListener,
    grpc: TcpListener,
    admin: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(http, grpc, Some(admin), config, shutdown).await
}

async fn launch(
    http: TcpListener,
    grpc: TcpListener,
    admin: Option<TcpListener>,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    let lifecycle = config.lifecycle(&shutdown);
    let mut background = Background::new(config.trail.clone());
    let (tls, log, wait) = (config.tls.clone(), config.log.clone(), config.timing.wait);
    let shared = build(config, &lifecycle, &mut background).await?;
    let auth = AuthServer::new(AuthController::new(Arc::clone(&shared.service)));
    let web = http_engine::Server::with_handler(app(Arc::clone(&shared)))
        .config(shared.config.http.clone());
    drop(shared);
    let admin = Admin::start(admin, &lifecycle);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    tokio::spawn(unready_on_stop::<Served>(
        reporter,
        lifecycle.health().clone(),
    ));
    let closing = lifecycle.closing();
    let (engine, edge) = match &tls {
        Some(tls) => {
            let inner = TcpListener::bind("127.0.0.1:0").await.map_err(Error::Io)?;
            let inner_addr = inner.local_addr().map_err(Error::Io)?;
            let config =
                mutual_server_config(&tls.cert, tls.key.expose(), &tls.ca).map_err(Error::Certs)?;
            let edge = terminate(http, inner_addr, config, closing.clone());
            (inner, Some(edge))
        }
        None => (http, None),
    };
    let builder = match &tls {
        Some(tls) => tonic::transport::Server::builder()
            .tls_config(tls.server())
            .map_err(Error::Tls)?,
        None => tonic::transport::Server::builder(),
    };
    let web_closing = closing.clone();
    let web = async move {
        web.serve(engine, async move { web_closing.cancelled().await })
            .await
            .map_err(Error::Io)
    };
    let edge = async move {
        match edge {
            Some(edge) => edge.await.map_err(Error::Io),
            None => Ok(()),
        }
    };
    let rpc = async move {
        builder
            .layer(AccessLog::new(NAME, log))
            .add_service(health)
            .add_service(auth)
            .serve_with_incoming_shutdown(TcpListenerStream::new(grpc), async move {
                closing.cancelled().await;
            })
            .await
            .map_err(Error::Serve)
    };
    let serving = async move { tokio::try_join!(web, edge, rpc).map(|_| ()) };
    let halted = lifecycle.run(wait, serving, background).await;
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}
