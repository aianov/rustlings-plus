mod memory;
mod state;

use std::future::Future;
use std::pin::Pin;

use contracts::UserId;

use crate::common::{Change, Consumed, Cursor, FeedItem};

pub use memory::{MemoryFeed, Snapshot};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait FeedRepository: Send + Sync {
    fn apply(&self, aggregate: String, version: u64, change: Change) -> BoxFuture<'_, Consumed>;
    fn page(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<FeedItem>>;
    fn after(
        &self,
        viewer: UserId,
        before: Option<Cursor>,
        take: usize,
    ) -> BoxFuture<'_, (Vec<FeedItem>, Option<Cursor>)>;
    fn drain(&self, budget: usize) -> BoxFuture<'_, usize>;
    fn pending(&self) -> BoxFuture<'_, usize>;
    fn snapshot(&self) -> BoxFuture<'_, Snapshot>;
}
