mod memory;

use std::future::Future;
use std::pin::Pin;

use contracts::UserId;
use journal::JournalError;

use crate::common::Message;

pub use memory::{Line, MemoryChats};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ChatRepository: Send + Sync {
    fn append(&self, message: Message) -> BoxFuture<'_, Result<Message, JournalError>>;
    fn history(&self, user: UserId, peer: UserId, limit: usize) -> BoxFuture<'_, Vec<Message>>;
}
