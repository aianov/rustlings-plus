use std::future::Future;
use std::io;
use tokio::net::TcpListener;

pub async fn run(listener: TcpListener, max_connections: usize, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    todo!()
}
