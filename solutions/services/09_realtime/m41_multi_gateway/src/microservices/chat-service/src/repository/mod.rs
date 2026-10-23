mod memory;
mod outbox;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Cause, OutboxRow, UserId};
use journal::JournalError;
use tokio::sync::Notify;

use crate::common::Message;

pub use memory::{Line, MemoryChats};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ChatRepository: Send + Sync {
    fn append(
        &self,
        message: Message,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Message, JournalError>>;
    fn history(&self, user: UserId, peer: UserId, limit: usize) -> BoxFuture<'_, Vec<Message>>;
}

pub trait OutboxRepository: Send + Sync {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>>;
    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>>;
    fn waiting(&self) -> BoxFuture<'_, usize>;
    fn signal(&self) -> Arc<Notify>;
}
