use std::env;
use std::process::ExitCode;

use api_gateway::{ADDR_ENV, Config, DEFAULT_ADDR, Error, run};
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

async fn serve() -> Result<(), Error> {
    let addr = env::var(ADDR_ENV).unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|source| Error::Bind { addr, source })?;
    let local = listener.local_addr().map_err(Error::Io)?;
    println!("api-gateway слушает {local}, остановка по Ctrl-C");
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    run(listener, Config::from_env(), shutdown).await
}
