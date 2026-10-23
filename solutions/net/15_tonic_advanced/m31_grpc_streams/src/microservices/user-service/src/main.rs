use std::process::ExitCode;
use std::time::Duration;

use grpc_kit::LogSink;
use proto::USERS;
use service_runtime::CancellationToken;
use social_core::report;
use tokio::net::TcpListener;
use user_service::{Config, Error, run};

#[tokio::main]
async fn main() -> ExitCode {
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("user-service: {}", report(&error));
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> Result<(), Error> {
    let addr = USERS.addr();
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|source| Error::Bind { addr, source })?;
    let local = listener.local_addr().map_err(Error::Io)?;
    println!("user-service слушает gRPC на {local}, остановка по Ctrl-C");
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    let config = Config {
        log: Some(LogSink::stderr()),
        grace: Duration::from_secs(1),
        ..Config::from_env()
    };
    run(listener, config, shutdown).await
}
