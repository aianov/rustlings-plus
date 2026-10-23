use std::process::ExitCode;

use auth_service::{Config, Error, NAME, serve};
use grpc_kit::Tls;
use service_runtime::{Signals, Source, report};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    service_runtime::init_logging(NAME);
    match start().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{NAME}: {}", report(&error));
            ExitCode::FAILURE
        }
    }
}

async fn bind(addr: &str) -> Result<TcpListener, Error> {
    TcpListener::bind(addr).await.map_err(|source| Error::Bind {
        addr: addr.to_string(),
        source,
    })
}

async fn start() -> Result<(), Error> {
    let signals = Signals::listen().map_err(Error::Io)?;
    let config = Config::from_source(&Source::load(".env")?)?;
    let tls = Tls::load(&config.certs, NAME).map_err(Error::Certs)?;
    let http = bind(&config.http_addr).await?;
    let grpc = bind(&config.addr).await?;
    let admin = bind(&config.admin).await?;
    let web = http.local_addr().map_err(Error::Io)?;
    let rpc = grpc.local_addr().map_err(Error::Io)?;
    let ops = admin.local_addr().map_err(Error::Io)?;
    tracing::info!(
        "{NAME}: HTTP входа на https://{web}/auth только для шлюза по mTLS, gRPC с mTLS на {rpc}, admin http://{ops}/ready, остановка по SIGTERM или Ctrl-C"
    );
    let shutdown = signals.shutdown(config.trail.clone());
    let config = Config {
        tls: Some(tls),
        ..config
    };
    serve(http, grpc, admin, config, shutdown).await
}
