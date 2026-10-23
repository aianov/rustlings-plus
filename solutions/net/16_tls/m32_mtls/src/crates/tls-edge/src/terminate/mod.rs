use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use service_runtime::CancellationToken;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tokio_rustls::TlsAcceptor;

pub const HANDSHAKE: Duration = Duration::from_secs(10);

pub const DRAIN: Duration = Duration::from_secs(5);

async fn relay(tcp: TcpStream, inner: SocketAddr, acceptor: TlsAcceptor) {
    let _ = tcp.set_nodelay(true);
    let Ok(Ok(mut tls)) = timeout(HANDSHAKE, acceptor.accept(tcp)).await else {
        return;
    };
    let Ok(mut plain) = TcpStream::connect(inner).await else {
        return;
    };
    let _ = plain.set_nodelay(true);
    let _ = copy_bidirectional(&mut tls, &mut plain).await;
}

pub async fn terminate(
    public: TcpListener,
    inner: SocketAddr,
    config: Arc<ServerConfig>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let acceptor = TlsAcceptor::from(config);
    let mut relays = JoinSet::new();
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = public.accept() => match accepted {
                Ok((tcp, _)) => {
                    relays.spawn(relay(tcp, inner, acceptor.clone()));
                }
                Err(..) => sleep(Duration::from_millis(10)).await,
            },
            Some(..) = relays.join_next(), if !relays.is_empty() => {}
        }
    }
    drop(public);
    let drained = async { while relays.join_next().await.is_some() {} };
    if timeout(DRAIN, drained).await.is_err() {
        relays.abort_all();
    }
    Ok(())
}
