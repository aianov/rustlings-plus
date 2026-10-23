mod wire;

pub use wire::{Frame, FrameReader, encode};

use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[derive(Debug)]
pub enum RpcError {
    Closed,
    Io(io::Error),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::Closed => f.write_str("соединение закрыто"),
            RpcError::Io(e) => write!(f, "ошибка ввода-вывода: {e}"),
        }
    }
}

impl std::error::Error for RpcError {}

#[derive(Clone)]
pub struct Client {}

impl Client {
    pub async fn connect(addr: SocketAddr) -> io::Result<Client> {
        todo!()
    }

    pub async fn call(&self, body: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        todo!()
    }

    pub fn pending(&self) -> usize {
        todo!()
    }
}

pub async fn serve<F, Fut>(listener: TcpListener, handler: F) -> io::Result<()>
where
    F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Vec<u8>> + Send + 'static,
{
    todo!()
}
