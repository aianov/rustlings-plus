mod config;
mod connection;
mod error;
mod hub;
mod protocol;
mod server;
mod session;

use std::sync::Arc;

use service_runtime::CancellationToken;
use social_core::{App, AppConfig, MemNetwork, Notifiers};
use tokio::net::TcpListener;

use server::Shared;

pub use config::{ADDR_ENV, Config, DEFAULT_ADDR, JOURNAL_ENV};
pub use error::{Error, Result};
pub use hub::{Hub, Link};
pub use session::{Session, SessionError};

pub async fn run(listener: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
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
        config,
    });
    server::accept(listener, shared, shutdown).await;
    app.close(drain).await;
    Ok(())
}
