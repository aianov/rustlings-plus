// Обработчик публикации делает три шага, и если клиент ушёл посреди второго, пост навсегда
// остаётся «в ожидании». Реши, что должно доделываться несмотря ни на что, и как этого добиться.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("publisher.v1");
}
use pb::publisher_server::{Publisher, PublisherServer};
use pb::{PublishReply, PublishRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Published,
}

#[derive(Default)]
pub struct Store {
    entries: Mutex<Vec<(String, State)>>,
    notified: AtomicUsize,
}

impl Store {
    pub fn new() -> Arc<Store> {
        Arc::default()
    }

    pub fn snapshot(&self) -> Vec<(String, State)> {
        self.entries.lock().unwrap().clone()
    }

    pub fn notified(&self) -> usize {
        self.notified.load(Ordering::SeqCst)
    }

    fn add_pending(&self, text: &str) -> u64 {
        let mut entries = self.entries.lock().unwrap();
        entries.push((text.to_string(), State::Pending));
        entries.len() as u64
    }

    fn mark_published(&self, id: u64) {
        self.entries.lock().unwrap()[id as usize - 1].1 = State::Published;
    }

    async fn notify_followers(&self, text: &str) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if text == "boom" {
            panic!("упал сервис рассылки");
        }
        self.notified.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct PublisherSvc {
    store: Arc<Store>,
}

#[tonic::async_trait]
impl Publisher for PublisherSvc {
    async fn publish(&self, request: Request<PublishRequest>) -> Result<Response<PublishReply>, Status> {
        let text = request.into_inner().text;
        let id = self.store.add_pending(&text);
        self.store.notify_followers(&text).await;
        self.store.mark_published(id);
        Ok(Response::new(PublishReply { id }))
    }
}

pub async fn start(store: Arc<Store>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(PublisherServer::new(PublisherSvc { store })).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
