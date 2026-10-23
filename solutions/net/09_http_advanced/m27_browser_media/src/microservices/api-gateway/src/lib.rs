mod auth;
mod config;
mod connection;
mod error;
mod http;
mod hub;
mod media;
mod protocol;
mod server;
mod session;

use std::sync::Arc;

use http_engine::Server;
use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;

use server::Shared;

pub use auth::{TOKEN_BYTES, Tokens};
pub use config::{
    ADDR_ENV, CORS_ENV, Config, DEFAULT_ADDR, DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, JOURNAL_ENV,
};
pub use error::{Error, Result};
pub use hub::{Hub, Link};
pub use media::{Avatar, Avatars};
pub use session::{Session, SessionError};

pub async fn run(
    tcp: TcpListener,
    http: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
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
        config,
    });
    let api =
        Server::with_handler(http::app(Arc::clone(&shared))).config(shared.config.http.clone());
    let stop = shutdown.clone();
    let (_, served) = tokio::join!(
        server::accept(tcp, shared, shutdown),
        api.serve(http, async move { stop.cancelled().await })
    );
    app.close(drain).await;
    served.map_err(Error::Io)
}
