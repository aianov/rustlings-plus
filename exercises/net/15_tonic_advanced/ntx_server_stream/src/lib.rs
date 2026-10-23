// Сервер пишет клиенту, пока вызов открыт. Нужно связать подписку на хаб с потоком ответа так, чтобы
// ничего не терялось и ничего не утекало, когда клиент ушёл. Допиши subscribe.
#![allow(unused_variables)]

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("feed.v1");
}
use pb::feed_server::{Feed, FeedServer};
use pb::{Event, SubscribeRequest};

pub struct Hub {
    tx: broadcast::Sender<Event>,
}

impl Hub {
    pub fn new(capacity: usize) -> Arc<Hub> {
        Arc::new(Hub { tx: broadcast::channel(capacity).0 })
    }

    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    pub fn subscribers(&self) -> usize {
        self.tx.receiver_count()
    }

    fn listen(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

pub struct FeedSvc {
    hub: Arc<Hub>,
}

#[tonic::async_trait]
impl Feed for FeedSvc {
    type SubscribeStream = ReceiverStream<Result<Event, Status>>;

    async fn subscribe(&self, request: Request<SubscribeRequest>) -> Result<Response<Self::SubscribeStream>, Status> {
        todo!()
    }
}

pub async fn start(hub: Arc<Hub>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(FeedServer::new(FeedSvc { hub })).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
