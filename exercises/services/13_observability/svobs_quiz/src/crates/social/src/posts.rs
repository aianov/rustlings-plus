use crate::{Bus, Headers, Ids, Message};
use obs::TRACEPARENT;
use obs::metrics::Registry;
use obs::red::Red;
use obs::trace::enter;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{Instrument, info};

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    Broken,
}

impl PostError {
    pub fn status(&self) -> u16 {
        match self {
            PostError::Empty => 422,
            PostError::Broken => 500,
        }
    }
}

pub struct PostService {
    registry: Arc<Registry>,
    red: Red,
    bus: Bus,
    ids: Ids,
    next_post: AtomicU64,
}

impl PostService {
    pub fn new(bus: Bus, ids: Ids) -> PostService {
        let registry = Arc::new(Registry::new());
        PostService { red: Red::new(registry.clone()), registry, bus, ids, next_post: AtomicU64::new(1) }
    }

    pub fn metrics(&self) -> String {
        self.registry.render()
    }

    pub async fn create(&self, metadata: &Headers, body: &str) -> Result<u64, PostError> {
        let _ = (metadata, body);
        todo!()
    }

    async fn save(&self, body: &str) -> Result<u64, PostError> {
        match body {
            "" => Err(PostError::Empty),
            "boom" => Err(PostError::Broken),
            "slow" => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(self.next_post.fetch_add(1, Ordering::SeqCst))
            }
            _ => Ok(self.next_post.fetch_add(1, Ordering::SeqCst)),
        }
    }
}
