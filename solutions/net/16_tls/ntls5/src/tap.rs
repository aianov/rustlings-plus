use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};

pub struct Tap {
    pub addr: SocketAddr,
    to_server: Arc<Mutex<Vec<u8>>>,
    to_client: Arc<Mutex<Vec<u8>>>,
}

impl Tap {
    pub async fn start(target: SocketAddr) -> Tap {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт для прослушки");
        let addr = listener.local_addr().expect("адрес прослушки");
        let to_server = Arc::new(Mutex::new(Vec::new()));
        let to_client = Arc::new(Mutex::new(Vec::new()));
        let (up, down) = (to_server.clone(), to_client.clone());
        tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else { continue };
                let Ok(server) = TcpStream::connect(target).await else { continue };
                let (cr, cw) = client.into_split();
                let (sr, sw) = server.into_split();
                tokio::spawn(pump(cr, sw, up.clone()));
                tokio::spawn(pump(sr, cw, down.clone()));
            }
        });
        Tap { addr, to_server, to_client }
    }

    pub fn captured_to_server(&self) -> Vec<u8> {
        self.to_server.lock().unwrap().clone()
    }

    pub fn captured_to_client(&self) -> Vec<u8> {
        self.to_client.lock().unwrap().clone()
    }
}

async fn pump(mut from: OwnedReadHalf, mut to: OwnedWriteHalf, log: Arc<Mutex<Vec<u8>>>) {
    let mut buf = [0u8; 4096];
    loop {
        match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                log.lock().unwrap().extend_from_slice(&buf[..n]);
                if to.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown().await;
}
