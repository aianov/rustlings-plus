use std::process::ExitCode;

use grpc_kit::Tls;
use service_runtime::{Signals, Source, report};
use tokio::net::TcpListener;
use user_service::{Config, Error, NAME, serve};

#[tokio::main]
async fn main() -> ExitCode {
    match start().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{NAME}: {}", report(&error));
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
    let grpc = bind(&config.addr).await?;
    let admin = bind(&config.admin).await?;
    let rpc = grpc.local_addr().map_err(Error::Io)?;
    let web = admin.local_addr().map_err(Error::Io)?;
    println!("{NAME}: gRPC на {rpc}, admin http://{web}/ready, остановка по SIGTERM или Ctrl-C");
    let shutdown = signals.shutdown(config.trail.clone());
    let config = Config {
        tls: Some(tls),
        ..config
    };
    serve(grpc, admin, config, shutdown).await
}
