use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use service_runtime::CancellationToken;
use tokio::io::{AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};
use tokio_rustls::TlsAcceptor;

pub const HANDSHAKE: Duration = Duration::from_secs(10);

pub const DRAIN: Duration = Duration::from_secs(5);

fn announcement(client: SocketAddr, local: SocketAddr) -> String {
    let family = match (client, local) {
        (SocketAddr::V4(..), SocketAddr::V4(..)) => "TCP4",
        (SocketAddr::V6(..), SocketAddr::V6(..)) => "TCP6",
        _ => return String::from("PROXY UNKNOWN\r\n"),
    };
    format!(
        "PROXY {family} {} {} {} {}\r\n",
        client.ip(),
        local.ip(),
        client.port(),
        local.port()
    )
}

async fn relay(tcp: TcpStream, inner: SocketAddr, acceptor: TlsAcceptor, announce: bool) {
    let _ = tcp.set_nodelay(true);
    let line = match (tcp.peer_addr(), tcp.local_addr()) {
        (Ok(client), Ok(local)) => announcement(client, local),
        _ => String::from("PROXY UNKNOWN\r\n"),
    };
    let Ok(Ok(mut tls)) = timeout(HANDSHAKE, acceptor.accept(tcp)).await else {
        return;
    };
    let Ok(mut plain) = TcpStream::connect(inner).await else {
        return;
    };
    let _ = plain.set_nodelay(true);
    if announce && plain.write_all(line.as_bytes()).await.is_err() {
        return;
    }
    let _ = copy_bidirectional(&mut tls, &mut plain).await;
}

pub async fn terminate(
    public: TcpListener,
    inner: SocketAddr,
    config: Arc<ServerConfig>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    edge(public, inner, config, shutdown, false).await
}

pub async fn terminate_announced(
    public: TcpListener,
    inner: SocketAddr,
    config: Arc<ServerConfig>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    edge(public, inner, config, shutdown, true).await
}

async fn edge(
    public: TcpListener,
    inner: SocketAddr,
    config: Arc<ServerConfig>,
    shutdown: CancellationToken,
    announce: bool,
) -> io::Result<()> {
    let acceptor = TlsAcceptor::from(config);
    let mut relays = JoinSet::new();
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            accepted = public.accept() => match accepted {
                Ok((tcp, _)) => {
                    relays.spawn(relay(tcp, inner, acceptor.clone(), announce));
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
