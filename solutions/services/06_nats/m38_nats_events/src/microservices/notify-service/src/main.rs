use std::process::ExitCode;

use grpc_kit::Tls;
use notify_service::{Config, Error, NAME, serve};
use service_runtime::{Signals, Source, report};
use tokio::net::TcpListener;

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

async fn start() -> Result<(), Error> {
    let signals = Signals::listen().map_err(Error::Io)?;
    let config = Config::from_source(&Source::load(".env")?)?;
    let tls = Tls::load(&config.certs, NAME).map_err(Error::Certs)?;
    let admin = TcpListener::bind(&config.admin)
        .await
        .map_err(|source| Error::Bind {
            addr: config.admin.clone(),
            source,
        })?;
    let web = admin.local_addr().map_err(Error::Io)?;
    println!(
        "{NAME}: события из {} (корень {}), admin http://{web}/ready, остановка по SIGTERM или Ctrl-C",
        config.nats.url,
        config.topics.root()
    );
    let shutdown = signals.shutdown(config.trail.clone());
    let config = Config {
        tls: Some(tls),
        ..config
    };
    serve(admin, config, shutdown).await
}
