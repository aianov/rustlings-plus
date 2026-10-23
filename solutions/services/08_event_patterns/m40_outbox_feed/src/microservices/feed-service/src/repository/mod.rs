mod memory;

use std::future::Future;
use std::pin::Pin;

use contracts::UserId;

use crate::common::{Change, Consumed, FeedItem};

pub use memory::{MemoryFeed, Snapshot};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait FeedRepository: Send + Sync {
    fn apply(&self, aggregate: String, version: u64, change: Change) -> BoxFuture<'_, Consumed>;
    fn page(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<FeedItem>>;
    fn snapshot(&self) -> BoxFuture<'_, Snapshot>;
}
