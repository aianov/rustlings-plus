use crate::run::Target;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use tokio::net::TcpStream;

pub struct HttpTarget {
    addr: SocketAddr,
    path: String,
    conns: Mutex<HashMap<u64, TcpStream>>,
    next: AtomicU64,
}

impl HttpTarget {
    pub fn new(addr: SocketAddr, path: &str) -> HttpTarget {
        HttpTarget { addr, path: path.to_string(), conns: Mutex::new(HashMap::new()), next: AtomicU64::new(0) }
    }
}

impl Target for HttpTarget {
    fn connect(&self) -> BoxFuture<'_, Result<u64, String>> {
        let _ = (&self.addr, &self.conns, &self.next);
        todo!()
    }

    fn request(&self, conn: u64) -> BoxFuture<'_, Result<(), String>> {
        let _ = (conn, &self.path);
        todo!()
    }
}
