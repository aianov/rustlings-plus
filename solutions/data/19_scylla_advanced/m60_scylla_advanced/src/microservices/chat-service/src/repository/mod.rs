mod memory;
mod outbox;
mod scylla;
mod scylla_more;
mod scylla_pages;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Cause, OutboxRow, UserId};
use journal::JournalError;
use store_engine::StoreError;
use tokio::sync::{Notify, mpsc};

use crate::common::Message;

pub use memory::{Line, MemoryChats};
pub use scylla::{BUCKETS_BACK, SCHEMA, ScyllaChats, bucket_of, chat_of};
pub use scylla_more::UNREAD_CAP;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ChatRepository: Send + Sync {
    fn append(
        &self,
        message: Message,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Message, JournalError>>;
    fn history(&self, user: UserId, peer: UserId, limit: usize) -> BoxFuture<'_, Vec<Message>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSummary {
    pub peer: UserId,
    pub last_at: u64,
    pub preview: String,
    pub unread: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edited {
    Changed,
    Missing,
    NotAuthor,
}

pub trait ChatExtras: Send + Sync {
    fn send_for(
        &self,
        message: Message,
        ttl_seconds: u32,
    ) -> BoxFuture<'_, Result<Message, StoreError>>;
    fn edit(
        &self,
        user: UserId,
        id: String,
        text: String,
    ) -> BoxFuture<'_, Result<Edited, StoreError>>;
    fn mark_read(&self, user: UserId, peer: UserId) -> BoxFuture<'_, Result<(), StoreError>>;
    fn unread(&self, user: UserId, peer: UserId) -> BoxFuture<'_, Result<u32, StoreError>>;
}

pub type Page = (Vec<Message>, Option<String>);

pub trait ChatPages: Send + Sync {
    fn page(
        &self,
        user: UserId,
        peer: UserId,
        cursor: Option<String>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Page, StoreError>>;
    fn chats(
        &self,
        user: UserId,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<ChatSummary>, StoreError>>;
    fn export(
        &self,
        user: UserId,
        peer: UserId,
        sink: mpsc::Sender<Message>,
    ) -> BoxFuture<'_, Result<usize, StoreError>>;
}

pub trait OutboxRepository: Send + Sync {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>>;
    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>>;
    fn waiting(&self) -> BoxFuture<'_, usize>;
    fn signal(&self) -> Arc<Notify>;
}
