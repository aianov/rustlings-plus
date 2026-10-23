mod auth;
mod config;
mod error;
mod http;
mod inbox;
mod protocol;
mod registry;
mod routes;
mod state;
mod upstream;
mod ws;

use std::sync::Arc;

use http_engine::Server;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use state::Shared;

pub use auth::RemoteAuth;
pub use config::{BUDGET, CORS_ENV, Config, DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, Services};
pub use error::{Error, Result, report};
pub use inbox::{InboxGuard, Inboxes, OPEN_WAIT};
pub use registry::{ConnId, Out, Outbox, Registration, Registry, TOO_SLOW};
pub use routes::{Answer, Ctx, Pending, Routes, Watching};
pub use upstream::{Stamped, Upstreams};
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
    let upstreams = Upstreams::connect(&config.services)?;
    let registry = Arc::new(Registry::default());
    let shared = Arc::new(Shared {
        auth: RemoteAuth::new(upstreams.clone()),
        inboxes: Arc::new(Inboxes::new(upstreams.clone(), Arc::clone(&registry))),
        readiness: upstreams.readiness(&shutdown),
        upstreams,
        registry,
        routes,
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
    served.map_err(Error::Io)
}
