use std::env;
use std::process::ExitCode;

use api_gateway::{Config, DEFAULT_HTTP_ADDR, Error, HTTP_ADDR_ENV, run};
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
    let http = bind(HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR).await?;
    let web = http.local_addr().map_err(Error::Io)?;
    println!("api-gateway слушает http://{web} и ws://{web}/ws, остановка по Ctrl-C");
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    let config = Config {
        access_log: true,
        ..Config::from_env()
    };
    run(http, config, shutdown).await
}
