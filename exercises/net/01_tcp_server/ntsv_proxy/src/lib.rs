use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::net::TcpListener;

#[derive(Default)]
pub struct Totals {
    pub connections: AtomicU64,
    pub failed: AtomicU64,
    pub up: AtomicU64,
    pub down: AtomicU64,
}

pub async fn run(listener: TcpListener, upstream: SocketAddr, totals: Arc<Totals>, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    todo!()
}
