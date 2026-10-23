use std::env;
use std::process::ExitCode;

use api_gateway::{ADDR_ENV, Config, DEFAULT_ADDR, DEFAULT_HTTP_ADDR, Error, HTTP_ADDR_ENV, run};
use service_runtime::CancellationToken;
use social_core::report;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ошибка: {}", report(&error));
            ExitCode::FAILURE
        }
    }
}

async fn bind(key: &str, default: &str) -> Result<TcpListener, Error> {
    let addr = env::var(key).unwrap_or_else(|_| default.to_string());
    TcpListener::bind(&addr)
        .await
        .map_err(|source| Error::Bind { addr, source })
}

async fn serve() -> Result<(), Error> {
    let tcp = bind(ADDR_ENV, DEFAULT_ADDR).await?;
    let http = bind(HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR).await?;
    let local = tcp.local_addr().map_err(Error::Io)?;
    let web = http.local_addr().map_err(Error::Io)?;
    println!("api-gateway слушает {local} (NDJSON) и http://{web}, остановка по Ctrl-C");
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    run(tcp, http, Config::from_env(), shutdown).await
}
