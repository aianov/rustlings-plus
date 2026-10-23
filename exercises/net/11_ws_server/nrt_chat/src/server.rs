use std::future::Future;
use std::io;
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::{Config, Stats, Tokens};
pub async fn run<F>(listener: TcpListener, tokens: Arc<dyn Tokens>, config: Config, stats: Arc<Stats>, shutdown: F) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    todo!()
}
