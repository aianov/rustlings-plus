mod memory;

use std::future::Future;
use std::pin::Pin;

use contracts::UserId;

use crate::common::{Draft, Notice};

pub use memory::MemoryNotices;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    Stored(Notice),
    Duplicate,
}

pub trait NoticeRepository: Send + Sync {
    fn record(&self, draft: Draft) -> BoxFuture<'_, Recorded>;
    fn unread(&self, user: UserId) -> BoxFuture<'_, u32>;
    fn list(&self, user: UserId, limit: usize) -> BoxFuture<'_, Vec<Notice>>;
    fn mark_read(&self, user: UserId) -> BoxFuture<'_, u32>;
    fn unread_counts(&self) -> BoxFuture<'_, Vec<(UserId, u32)>>;
}
