use std::process::ExitCode;
use std::time::Duration;

use grpc_kit::{LogSink, Tls};
use post_service::{Config, Error, run};
use proto::POSTS;
use service_runtime::{CancellationToken, report};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("post-service: {}", report(&error));
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> Result<(), Error> {
    let addr = POSTS.addr();
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|source| Error::Bind { addr, source })?;
    let local = listener.local_addr().map_err(Error::Io)?;
    let tls = Tls::from_env("post-service").map_err(Error::Certs)?;
    let config = Config {
        log: Some(LogSink::stderr()),
        tls: Some(tls),
        grace: Duration::from_secs(1),
        ..Config::from_env()
    };
    println!(
        "post-service слушает gRPC на {local}, профили берёт из {}, остановка по Ctrl-C",
        config.users
    );
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            stop.cancel();
        }
    });
    run(listener, config, shutdown).await
}
