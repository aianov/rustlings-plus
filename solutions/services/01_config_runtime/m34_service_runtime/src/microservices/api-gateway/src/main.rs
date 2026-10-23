use std::process::ExitCode;

use api_gateway::{Config, Error, NAME, serve};
use grpc_kit::Tls;
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
    let admin = bind(&config.admin).await?;
    let web = http.local_addr().map_err(Error::Io)?;
    let ops = admin.local_addr().map_err(Error::Io)?;
    let services = &config.services;
    println!(
        "{NAME}: https://{web} и wss://{web}/ws, admin http://{ops}/ready, сервисы по mTLS: auth {}, users {}, posts {}, chat {}; остановка по SIGTERM или Ctrl-C",
        services.auth, services.users, services.posts, services.chat
    );
    let shutdown = signals.shutdown(config.trail.clone());
    let config = Config {
        tls: Some(tls),
        ..config
    };
    serve(http, admin, config, shutdown).await
}
