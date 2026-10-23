mod accounts;
mod config;
mod error;
mod grpc;
mod http;
mod state;
mod tokens;

use std::sync::Arc;

use grpc_kit::{AccessLog, HARD_STOP, bounded, channel, drain};
use proto::social::v1::auth_server::AuthServer;
use service_runtime::CancellationToken;
use tls_edge::{server_config, terminate};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic_health::server::health_reporter;

use state::Shared;

pub use accounts::{Account, Accounts};
pub use config::{CORS_ENV, Config, DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, JOURNAL_ENV};
pub use error::{Error, Result};
pub use grpc::AuthService;
pub use http::{SESSION_COOKIE, SESSION_TTL};
pub use tokens::{TOKEN_BYTES, Tokens};

type Served = AuthServer<AuthService>;

pub async fn run(
    http: TcpListener,
    grpc: TcpListener,
    config: Config,
    shutdown: CancellationToken,
) -> Result<()> {
    let users =
        channel(&config.users, "user-service", config.tls.as_ref()).map_err(Error::Upstream)?;
    let tls = config.tls.clone();
    let accounts = Arc::new(Accounts::open(config.journal.as_deref()).await?);
    let tokens = Arc::new(Tokens::default());
    let auth = AuthServer::new(AuthService::new(Arc::clone(&accounts), Arc::clone(&tokens)));
    let shared = Arc::new(Shared {
        accounts,
        tokens,
        users,
        config,
    });
    let web = http_engine::Server::with_handler(http::app(Arc::clone(&shared)))
        .config(shared.config.http.clone());
    let (grace, log) = (shared.config.grace, shared.config.log.clone());
    drop(shared);
    let (reporter, health) = health_reporter();
    reporter.set_serving::<Served>().await;
    let stop = shutdown.child();
    let (web_stop, rpc_stop, edge_stop) = (stop.clone(), stop.clone(), stop.clone());
    let (engine, edge) = match &tls {
        Some(tls) => {
            let inner = TcpListener::bind("127.0.0.1:0").await.map_err(Error::Io)?;
            let inner_addr = inner.local_addr().map_err(Error::Io)?;
            let config = server_config(&tls.cert, &tls.key).map_err(Error::Certs)?;
            (inner, Some(terminate(http, inner_addr, config, edge_stop)))
        }
        None => (http, None),
    };
    let builder = match &tls {
        Some(tls) => tonic::transport::Server::builder()
            .tls_config(tls.server())
            .map_err(Error::Tls)?,
        None => tonic::transport::Server::builder(),
    };
    let web = async move {
        let served = web
            .serve(engine, async move { web_stop.cancelled().await })
            .await;
        stop.cancel();
        served.map_err(Error::Io)
    };
    let edge = async move {
        match edge {
            Some(edge) => edge.await.map_err(Error::Io),
            None => Ok(()),
        }
    };
    let rpc = async move {
        let serving = builder
            .layer(AccessLog::new("auth-service", log))
            .add_service(health)
            .add_service(auth)
            .serve_with_incoming_shutdown(
                TcpListenerStream::new(grpc),
                drain::<Served>(reporter, rpc_stop.clone(), grace),
            );
        let served = bounded(serving, rpc_stop.clone(), grace + HARD_STOP).await;
        rpc_stop.cancel();
        served.map_err(Error::Serve)
    };
    let (web, edge, rpc) = tokio::join!(web, edge, rpc);
    web.and(edge).and(rpc)
}
