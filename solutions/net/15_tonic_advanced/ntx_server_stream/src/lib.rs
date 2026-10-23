// Сервер пишет клиенту, пока вызов открыт. Нужно связать подписку на хаб с потоком ответа так, чтобы
// ничего не терялось и ничего не утекало, когда клиент ушёл. Допиши subscribe.
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
        let user_id = request.into_inner().user_id;
        let mut events = self.hub.listen();
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tx.closed() => break,
                    received = events.recv() => match received {
                        Ok(event) if event.user_id == user_id => {
                            if tx.send(Ok(event)).await.is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(missed)) => {
                            let _ = tx.send(Err(Status::data_loss(format!("пропущено событий: {missed}")))).await;
                            break;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
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
