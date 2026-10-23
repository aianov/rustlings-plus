mod state;

use std::sync::Arc;

use http_engine::Server;
use service_runtime::CancellationToken;
use tls_edge::{server_config, terminate};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::client::{Inboxes, RemoteAuth, Upstreams};
use crate::common::{Config, Error, Registry, Result};
use crate::controller;
use crate::routes::Routes;

pub(crate) use state::Shared;

pub async fn run(http: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    run_with(http, config, Routes::social(), shutdown).await
}

pub async fn run_with(
    http: TcpListener,
    config: Config,
    routes: Routes,
    shutdown: CancellationToken,
) -> Result<()> {
    let upstreams = Upstreams::connect(&config.services, config.tls.as_ref())?;
    let (engine, edge) = match &config.tls {
        Some(tls) => {
            let inner = TcpListener::bind("127.0.0.1:0").await.map_err(Error::Io)?;
            let inner_addr = inner.local_addr().map_err(Error::Io)?;
            let edge_config = server_config(&tls.cert, &tls.key).map_err(Error::Certs)?;
            let edge = terminate(http, inner_addr, edge_config, shutdown.clone());
            (inner, Some(edge))
        }
        None => (http, None),
    };
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
    let api = Server::with_handler(controller::app(Arc::clone(&shared)))
        .config(shared.config.http.clone());
    drop(shared);
    let web = api.serve(engine, async move { shutdown.cancelled().await });
    let edge = async move {
        match edge {
            Some(edge) => edge.await,
            None => Ok(()),
        }
    };
    let (served, edged) = tokio::join!(web, edge);
    served.and(edged).map_err(Error::Io)
}
