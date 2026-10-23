mod accounts;
mod config;
mod error;
mod grpc;
mod http;
mod state;
mod tokens;

use std::sync::Arc;

use proto::social::v1::auth_server::AuthServer;
use proto::social::v1::users_client::UsersClient;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Endpoint;

use state::Shared;

pub use accounts::{Account, Accounts};
pub use config::{CORS_ENV, Config, DEFAULT_HTTP_ADDR, HTTP_ADDR_ENV, JOURNAL_ENV};
pub use error::{Error, Result};
pub use grpc::AuthService;
pub use http::{SESSION_COOKIE, SESSION_TTL};
pub use tokens::{TOKEN_BYTES, Tokens};

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
        users: UsersClient::new(users),
        config,
    });
    let web = http_engine::Server::with_handler(http::app(Arc::clone(&shared)))
        .config(shared.config.http.clone());
    drop(shared);
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
        let served = tonic::transport::Server::builder()
            .add_service(auth)
            .serve_with_incoming_shutdown(TcpListenerStream::new(grpc), async {
                rpc_stop.cancelled().await
            })
            .await;
        rpc_stop.cancel();
        served.map_err(Error::Serve)
    };
    let (web, rpc) = tokio::join!(web, rpc);
    web.and(rpc)
}
