use std::io;
use std::time::Duration;

use http_engine::{Server, ServerConfig};
use service_runtime::{CancellationToken, Health, Lifecycle};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::routes::router;

pub fn admin_config() -> ServerConfig {
    ServerConfig {
        idle_timeout: Duration::from_secs(5),
        head_timeout: Duration::from_secs(2),
        body_timeout: Duration::from_secs(2),
        max_connections: 64,
        drain_timeout: Duration::from_secs(1),
        ..ServerConfig::default()
    }
}

pub async fn serve(
    listener: TcpListener,
    health: Health,
    stop: CancellationToken,
) -> io::Result<()> {
    Server::new(router(health))
        .config(admin_config())
        .serve(listener, async move { stop.cancelled().await })
        .await
}

pub struct Admin {
    stop: CancellationToken,
    task: Option<JoinHandle<io::Result<()>>>,
}

impl Admin {
    pub fn start(listener: Option<TcpListener>, lifecycle: &Lifecycle) -> Admin {
        let stop = lifecycle.finished();
        let health = lifecycle.health().clone();
        let task = listener.map(|listener| tokio::spawn(serve(listener, health, stop.clone())));
        Admin { stop, task }
    }

    pub async fn stop(self) -> io::Result<()> {
        self.stop.cancel();
        match self.task {
            Some(task) => task
                .await
                .unwrap_or_else(|error| Err(io::Error::other(error))),
            None => Ok(()),
        }
    }
}
