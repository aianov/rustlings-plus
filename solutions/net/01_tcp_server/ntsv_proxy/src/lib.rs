use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

#[derive(Default)]
pub struct Totals {
    pub connections: AtomicU64,
    pub failed: AtomicU64,
    pub up: AtomicU64,
    pub down: AtomicU64,
}

pub async fn run(listener: TcpListener, upstream: SocketAddr, totals: Arc<Totals>, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    tokio::pin!(shutdown);
    let mut conns = JoinSet::new();
    loop {
        while conns.try_join_next().is_some() {}
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((client, _)) => {
                    conns.spawn(proxy(client, upstream, totals.clone()));
                }
                Err(e) if is_temporary(&e) => tokio::time::sleep(Duration::from_millis(50)).await,
                Err(e) => return Err(e),
            },
        }
    }
    drop(listener);
    while conns.join_next().await.is_some() {}
    Ok(())
}

fn is_temporary(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(23 | 24))
        || matches!(e.kind(), io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset | io::ErrorKind::Interrupted)
}

async fn proxy(client: TcpStream, upstream: SocketAddr, totals: Arc<Totals>) {
    let Ok(server) = TcpStream::connect(upstream).await else {
        totals.failed.fetch_add(1, Relaxed);
        return;
    };
    totals.connections.fetch_add(1, Relaxed);
    let (client_read, client_write) = client.into_split();
    let (server_read, server_write) = server.into_split();
    let _ = tokio::try_join!(
        pump(client_read, server_write, &totals.up),
        pump(server_read, client_write, &totals.down),
    );
}

async fn pump(mut from: impl AsyncRead + Unpin, mut to: impl AsyncWrite + Unpin, counter: &AtomicU64) -> io::Result<()> {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = from.read(&mut buf).await?;
        if n == 0 {
            return to.shutdown().await;
        }
        to.write_all(&buf[..n]).await?;
        counter.fetch_add(n as u64, Relaxed);
    }
}
