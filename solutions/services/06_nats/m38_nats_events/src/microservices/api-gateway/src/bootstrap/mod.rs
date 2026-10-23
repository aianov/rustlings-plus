mod state;

use std::sync::Arc;

use admin::Admin;
use http_engine::{Server, ServerConfig};
use nats_engine::Bus;
use service_runtime::{Background, CancellationToken};
use tls_edge::{server_config, terminate_announced};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::client::{AuthHttp, Inboxes, Notify, RemoteAuth, Upstreams};
use crate::common::{Config, Error, Registry, Result};
use crate::controller;
use crate::routes::Routes;

pub(crate) use state::Shared;

pub async fn run(http: TcpListener, config: Config, shutdown: CancellationToken) -> Result<()> {
    launch(http, None, config, Routes::social(), shutdown).await
}

pub async fn run_with(
    http: TcpListener,
    config: Config,
    routes: Routes,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(http, None, config, routes, shutdown).await
}

pub async fn serve(
    http: TcpListener,
    admin: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    launch(http, Some(admin), config, Routes::social(), shutdown).await
}

async fn launch(
    http: TcpListener,
    admin: Option<TcpListener>,
    config: Config,
    routes: Routes,
    shutdown: CancellationToken,
) -> Result<()> {
    let router = routes.build().map_err(Error::Routes)?;
    let lifecycle = config.lifecycle(&shutdown);
    let mut background = Background::new(config.trail.clone());
    let upstreams = Upstreams::connect(&config.services, config.tls.as_ref())?;
    let auth_http = AuthHttp::new(
        &config.services.auth_http,
        config.tls.as_ref(),
        &config.proxy,
    )?;
    let readiness = upstreams.readiness(&mut background);
    readiness.register(lifecycle.health());
    let closing = lifecycle.closing();
    let (engine, edge) = match &config.tls {
        Some(tls) => {
            let inner = TcpListener::bind("127.0.0.1:0").await.map_err(Error::Io)?;
            let inner_addr = inner.local_addr().map_err(Error::Io)?;
            let edge_config = server_config(&tls.cert, tls.key.expose()).map_err(Error::Certs)?;
            let edge = terminate_announced(http, inner_addr, edge_config, closing.clone());
            (inner, Some(edge))
        }
        None => (http, None),
    };
    let wait = config.timing.wait;
    let registry = Arc::new(Registry::default());
    let notify = match &config.nats {
        Some(nats) => {
            let bus = Bus::connect(nats.clone()).await.map_err(Error::Bus)?;
            let notices = controller::notices::subscribe(&bus, &config.topics)
                .await
                .map_err(Error::Bus)?;
            tokio::spawn(controller::notices::forward(
                notices,
                config.topics.clone(),
                Arc::clone(&registry),
                closing.clone(),
            ));
            Some(Notify::new(
                bus,
                config.topics.clone(),
                config.notify_timeout,
            ))
        }
        None => None,
    };
    let bus = notify.as_ref().map(|notify| notify.bus().clone());
    let shared = Arc::new(Shared {
        auth: RemoteAuth::new(upstreams.clone()),
        auth_http,
        inboxes: Arc::new(Inboxes::new(upstreams.clone(), Arc::clone(&registry))),
        notify,
        readiness,
        upstreams,
        registry,
        router,
        sessions: Arc::new(Semaphore::new(config.max_connections.max(1))),
        shutdown: closing.clone(),
        config,
    });
    let web_config = ServerConfig {
        proxy_protocol: edge.is_some(),
        ..shared.config.http.clone()
    };
    let api = Server::with_handler(controller::app(Arc::clone(&shared))).config(web_config);
    drop(shared);
    let admin = Admin::start(admin, &lifecycle);
    let web = async move {
        api.serve(engine, async move { closing.cancelled().await })
            .await
            .map_err(Error::Io)
    };
    let edge = async move {
        match edge {
            Some(edge) => edge.await.map_err(Error::Io),
            None => Ok(()),
        }
    };
    let serving = async move { tokio::try_join!(web, edge).map(|_| ()) };
    let halted = lifecycle.run(wait, serving, background).await;
    if let Some(bus) = bus {
        bus.close().await.map_err(Error::Bus)?;
    }
    admin.stop().await.map_err(Error::Io)?;
    halted.map_err(Error::from)
}
