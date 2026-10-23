mod wire;

pub use wire::{Frame, FrameReader, encode};

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};

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

type Pending = Mutex<Option<HashMap<u64, oneshot::Sender<Vec<u8>>>>>;

struct Shared {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    pending: Pending,
    next_id: AtomicU64,
}

struct Forget<'a> {
    shared: &'a Shared,
    id: u64,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        if let Some(pending) = self.shared.pending.lock().unwrap().as_mut() {
            pending.remove(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}

impl Client {
    pub async fn connect(addr: SocketAddr) -> io::Result<Client> {
        let (reader, writer) = TcpStream::connect(addr).await?.into_split();
        let shared = Arc::new(Shared {
            writer: tokio::sync::Mutex::new(writer),
            pending: Mutex::new(Some(HashMap::new())),
            next_id: AtomicU64::new(1),
        });
        let task_shared = shared.clone();
        tokio::spawn(async move {
            let mut frames = FrameReader::new(reader);
            while let Ok(Some(frame)) = frames.next().await {
                let waiter = task_shared.pending.lock().unwrap().as_mut().and_then(|p| p.remove(&frame.id));
                if let Some(waiter) = waiter {
                    let _ = waiter.send(frame.body);
                }
            }
            task_shared.pending.lock().unwrap().take();
        });
        Ok(Client { shared })
    }

    pub async fn call(&self, body: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        let id = self.shared.next_id.fetch_add(1, Relaxed);
        let (tx, rx) = oneshot::channel();
        match self.shared.pending.lock().unwrap().as_mut() {
            Some(pending) => {
                pending.insert(id, tx);
            }
            None => return Err(RpcError::Closed),
        }
        let _guard = Forget { shared: &self.shared, id };
        let sent = self.shared.writer.lock().await.write_all(&encode(&Frame { id, body })).await;
        if let Err(e) = sent {
            return Err(RpcError::Io(e));
        }
        rx.await.map_err(|_| RpcError::Closed)
    }

    pub fn pending(&self) -> usize {
        self.shared.pending.lock().unwrap().as_ref().map_or(0, |p| p.len())
    }
}

pub async fn serve<F, Fut>(listener: TcpListener, handler: F) -> io::Result<()>
where
    F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Vec<u8>> + Send + 'static,
{
    let handler = Arc::new(handler);
    loop {
        let (stream, _) = listener.accept().await?;
        let handler = handler.clone();
        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let (tx, mut rx) = mpsc::channel::<Frame>(64);
            let writer_task = tokio::spawn(async move {
                while let Some(frame) = rx.recv().await {
                    if writer.write_all(&encode(&frame)).await.is_err() {
                        return;
                    }
                }
            });
            let mut frames = FrameReader::new(reader);
            while let Ok(Some(request)) = frames.next().await {
                let (handler, tx) = (handler.clone(), tx.clone());
                tokio::spawn(async move {
                    let body = handler(request.body).await;
                    let _ = tx.send(Frame { id: request.id, body }).await;
                });
            }
            drop(tx);
            let _ = writer_task.await;
        });
    }
}
