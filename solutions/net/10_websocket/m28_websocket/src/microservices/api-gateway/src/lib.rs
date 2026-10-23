mod auth;
mod config;
mod error;
mod http;
mod hub;
mod media;
mod protocol;
mod session;
mod state;
mod ws;

use std::sync::Arc;

use http_engine::Server;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use state::Shared;

pub use auth::{TOKEN_BYTES, Tokens};
pub use config::{CORS_ENV, Config, DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, JOURNAL_ENV};
pub use error::{Error, Result};
pub use hub::{Hub, Link};
pub use media::{Avatar, Avatars};
pub use session::{Session, SessionError};
pub use ws::{GOING_AWAY, UNSUPPORTED};

pub async fn run(http: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    let hub = Arc::new(Hub::default());
    let mut notifiers = Notifiers::new();
    notifiers.add(hub.clone());
    let core = AppConfig::default();
    let app = match &config.journal {
        Some(path) => App::restore(MemNetwork::new(), notifiers, core, path).await?,
        None => App::start(MemNetwork::new(), notifiers, core)?,
    };
    let drain = config.drain;
    let shared = Arc::new(Shared {
        app: app.clone(),
        hub,
        tokens: Tokens::default(),
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
