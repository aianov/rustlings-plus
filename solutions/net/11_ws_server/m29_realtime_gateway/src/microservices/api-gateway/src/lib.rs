mod auth;
mod config;
mod error;
mod http;
mod media;
mod protocol;
mod registry;
mod routes;
mod state;
mod ws;

use std::sync::Arc;

use http_engine::Server;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use state::Shared;

pub use auth::{Authenticator, TOKEN_BYTES, Tokens};
pub use config::{CORS_ENV, Config, DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, JOURNAL_ENV};
pub use error::{Error, Result};
pub use media::{Avatar, Avatars};
pub use registry::{ConnId, Out, Outbox, Registration, Registry, TOO_SLOW};
pub use routes::{Answer, Ctx, Pending, Routes};
pub use ws::{GOING_AWAY, UNSUPPORTED};

pub async fn run(http: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    run_with(http, config, Routes::social(), shutdown).await
}

pub async fn run_with(
    http: TcpListener,
    config: Config,
    routes: Routes,
    shutdown: CancellationToken,
) -> Result<()> {
    let registry = Arc::new(Registry::default());
    let mut notifiers = Notifiers::new();
    notifiers.add(registry.clone());
    let core = AppConfig::default();
    let app = match &config.journal {
        Some(path) => App::restore(MemNetwork::new(), notifiers, core, path).await?,
        None => App::start(MemNetwork::new(), notifiers, core)?,
    };
    let drain = config.drain;
    let tokens = Arc::new(Tokens::default());
    let shared = Arc::new(Shared {
        app: app.clone(),
        registry,
        tokens: Arc::clone(&tokens),
        auth: tokens,
        routes,
        avatars: Avatars::default(),
        sessions: Arc::new(Semaphore::new(config.max_connections.max(1))),
        shutdown: shutdown.clone(),
        config,
    });
    let api =
        Server::with_handler(http::app(Arc::clone(&shared))).config(shared.config.http.clone());
    drop(shared);
    let served = api
        .serve(http, async move { shutdown.cancelled().await })
        .await;
    app.close(drain).await;
    served.map_err(Error::Io)
}
