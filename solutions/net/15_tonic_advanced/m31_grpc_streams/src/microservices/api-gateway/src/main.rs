use std::env;
use std::process::ExitCode;

use api_gateway::{Config, DEFAULT_HTTP_ADDR, Error, HTTP_ADDR_ENV, report, run};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("api-gateway: {}", report(&error));
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
    let config = Config {
        access_log: true,
        ..Config::from_env()
    };
    let services = &config.services;
    println!(
        "api-gateway слушает http://{web} и ws://{web}/ws, сервисы: auth {}, users {}, posts {}, chat {}; остановка по Ctrl-C",
        services.auth, services.users, services.posts, services.chat
    );
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    run(http, config, shutdown).await
}
