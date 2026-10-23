mod accounts;
mod config;
mod error;
mod grpc;
mod http;
mod state;
mod tokens;

use std::sync::Arc;

use grpc_kit::{AccessLog, HARD_STOP, bounded, drain};
use proto::social::v1::auth_server::AuthServer;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Endpoint;
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
    let users = Endpoint::from_shared(format!("http://{}", config.users))
        .map_err(Error::Upstream)?
        .connect_lazy();
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
    let (web_stop, rpc_stop) = (stop.clone(), stop.clone());
    let web = async move {
        let served = web
            .serve(http, async move { web_stop.cancelled().await })
            .await;
        stop.cancel();
        served.map_err(Error::Io)
    };
    let rpc = async move {
        let serving = tonic::transport::Server::builder()
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
    let (web, rpc) = tokio::join!(web, rpc);
    web.and(rpc)
}
