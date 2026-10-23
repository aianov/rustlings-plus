// Общий чат: каждый участник одновременно слушает и говорит. Эхо «одно входящее, одно исходящее»
// тут не годится. Допиши talk так, чтобы чтение и запись жили независимо.
#![allow(unused_variables)]

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

pub mod pb {
    tonic::include_proto!("chat.v1");
}
use pb::chat_in::Kind;
use pb::chat_server::{Chat, ChatServer};
use pb::{ChatIn, ChatOut};

pub struct Room {
    tx: broadcast::Sender<ChatOut>,
}

impl Room {
    pub fn new() -> Arc<Room> {
        Arc::new(Room { tx: broadcast::channel(64).0 })
    }

    fn subscribe(&self) -> broadcast::Receiver<ChatOut> {
        self.tx.subscribe()
    }

    fn say(&self, from: &str, text: String) {
        let _ = self.tx.send(ChatOut { from: from.to_string(), text });
    }
}

pub struct ChatSvc {
    room: Arc<Room>,
}

#[tonic::async_trait]
impl Chat for ChatSvc {
    type TalkStream = ReceiverStream<Result<ChatOut, Status>>;

    async fn talk(&self, request: Request<Streaming<ChatIn>>) -> Result<Response<Self::TalkStream>, Status> {
        todo!()
    }
}

pub async fn start(room: Arc<Room>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(ChatServer::new(ChatSvc { room })).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}
