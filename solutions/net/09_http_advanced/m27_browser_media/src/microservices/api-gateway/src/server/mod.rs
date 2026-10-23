use std::sync::Arc;

use service_runtime::{CancellationToken, TaskTracker};
use social_core::App;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::time::{sleep, timeout};

use crate::auth::Tokens;
use crate::config::Config;
use crate::connection;
use crate::hub::Hub;
use crate::media::Avatars;

pub(crate) struct Shared {
    pub(crate) app: App,
    pub(crate) hub: Arc<Hub>,
    pub(crate) tokens: Tokens,
    pub(crate) avatars: Avatars,
    pub(crate) config: Config,
}

pub(crate) async fn accept(
    listener: TcpListener,
    shared: Arc<Shared>,
    shutdown: CancellationToken,
) {
    let limit = Arc::new(Semaphore::new(shared.config.max_connections.max(1)));
    let connections = TaskTracker::new();
    let console = shared.app.console().clone();
    loop {
        let permit = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            permit = Arc::clone(&limit).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(..) => break,
            },
        };
        let accepted = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((stream, _)) => {
                let _ = stream.set_nodelay(true);
                let work = connection::serve(
                    stream,
                    Arc::clone(&shared),
                    shutdown.clone(),
                    connections.clone(),
                    permit,
                );
                connections.spawn(console.traced("gateway-connection", work));
            }
            Err(error) => {
                drop(permit);
                eprintln!("api-gateway: accept не удался ({error}), пауза");
                tokio::select! {
                    biased;
                    () = shutdown.cancelled() => break,
                    () = sleep(shared.config.accept_pause) => {}
                }
            }
        }
    }
    drop(listener);
    connections.close();
    if timeout(shared.config.drain, connections.wait())
        .await
        .is_err()
    {
        connections.abort_all();
        connections.wait().await;
    }
}
