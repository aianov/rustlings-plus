use std::env;
use std::process::ExitCode;
use std::time::Duration;

use auth_service::{Config, DEFAULT_HTTP_ADDR, Error, HTTP_ADDR_ENV, run};
use grpc_kit::{LogSink, Tls};
use proto::AUTH;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("auth-service: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn bind(addr: String) -> Result<TcpListener, Error> {
    TcpListener::bind(&addr)
        .await
        .map_err(|source| Error::Bind { addr, source })
}

async fn serve() -> Result<(), Error> {
    let http =
        bind(env::var(HTTP_ADDR_ENV).unwrap_or_else(|_| DEFAULT_HTTP_ADDR.to_string())).await?;
    let grpc = bind(AUTH.addr()).await?;
    let web = http.local_addr().map_err(Error::Io)?;
    let rpc = grpc.local_addr().map_err(Error::Io)?;
    println!("auth-service: вход на https://{web}/auth, gRPC с mTLS на {rpc}, остановка по Ctrl-C");
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    let tls = Tls::from_env("auth-service").map_err(Error::Certs)?;
    let config = Config {
        access_log: true,
        tls: Some(tls),
        log: Some(LogSink::stderr()),
        grace: Duration::from_secs(1),
        ..Config::from_env()
    };
    run(http, grpc, config, shutdown).await
}
