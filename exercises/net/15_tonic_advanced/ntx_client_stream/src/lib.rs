// Лайки приходят пачкой в одном вызове. Сервер читает поток по одному сообщению и применяет
// результат только после явного Commit: просто конец потока ещё не повод верить, что клиент дошёл до конца.
#![allow(unused_variables)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};
use tonic::{Request, Response, Status, Streaming};

pub mod pb {
    tonic::include_proto!("likes.v1");
}
use pb::like_request::Kind;
use pb::likes_client::LikesClient;
use pb::likes_server::{Likes, LikesServer};
use pb::{Commit, Like, LikeRequest, LikeSummary};

pub const MAX_BATCH: u64 = 1000;

#[derive(Default)]
pub struct LikeStore {
    counts: Mutex<HashMap<u64, u64>>,
}

impl LikeStore {
    pub fn new() -> Arc<LikeStore> {
        Arc::default()
    }

    pub fn count(&self, post_id: u64) -> u64 {
        self.counts.lock().unwrap().get(&post_id).copied().unwrap_or(0)
    }

    pub fn total(&self) -> u64 {
        self.counts.lock().unwrap().values().sum()
    }

    fn apply(&self, batch: HashMap<u64, u64>) {
        let mut counts = self.counts.lock().unwrap();
        for (post, n) in batch {
            *counts.entry(post).or_default() += n;
        }
    }
}

pub struct LikesSvc {
    store: Arc<LikeStore>,
}

#[tonic::async_trait]
impl Likes for LikesSvc {
    async fn send_batch(&self, request: Request<Streaming<LikeRequest>>) -> Result<Response<LikeSummary>, Status> {
        todo!()
    }
}

pub async fn start(store: Arc<LikeStore>) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = Server::builder().add_service(LikesServer::new(LikesSvc { store })).serve_with_incoming(TcpListenerStream::new(listener)).await;
    });
    Ok(addr)
}

pub async fn send_likes(client: &mut LikesClient<Channel>, post_ids: Vec<u64>) -> Result<LikeSummary, Status> {
    todo!()
}
