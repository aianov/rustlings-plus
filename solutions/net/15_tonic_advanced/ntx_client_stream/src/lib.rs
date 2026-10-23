// Лайки приходят пачкой в одном вызове. Сервер читает поток по одному сообщению и применяет
// результат только после явного Commit: просто конец потока ещё не повод верить, что клиент дошёл до конца.
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
        let mut stream = request.into_inner();
        let mut batch: HashMap<u64, u64> = HashMap::new();
        let mut accepted = 0u64;
        loop {
            match stream.message().await?.and_then(|m| m.kind) {
                Some(Kind::Like(like)) => {
                    if like.post_id == 0 {
                        return Err(Status::invalid_argument("post_id: нужен ненулевой номер поста"));
                    }
                    accepted += 1;
                    if accepted > MAX_BATCH {
                        return Err(Status::resource_exhausted(format!("в одной пачке не больше {MAX_BATCH} лайков")));
                    }
                    *batch.entry(like.post_id).or_default() += 1;
                }
                Some(Kind::Commit(_)) => break,
                None => return Err(Status::aborted("пачка не закончена: не было Commit")),
            }
        }
        let distinct_posts = batch.len() as u64;
        self.store.apply(batch);
        Ok(Response::new(LikeSummary { accepted, distinct_posts }))
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
    let likes = post_ids.into_iter().map(|post_id| LikeRequest { kind: Some(Kind::Like(Like { post_id })) });
    let commit = std::iter::once(LikeRequest { kind: Some(Kind::Commit(Commit {})) });
    Ok(client.send_batch(tokio_stream::iter(likes.chain(commit))).await?.into_inner())
}
