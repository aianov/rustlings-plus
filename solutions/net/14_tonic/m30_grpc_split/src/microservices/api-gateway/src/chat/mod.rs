use std::sync::{Mutex, MutexGuard, PoisonError};

use contracts::UserId;
use social_core::{Chats, DomainError, Message};

#[derive(Default)]
pub struct ChatStore {
    chats: Mutex<Chats>,
}

impl ChatStore {
    fn lock(&self) -> MutexGuard<'_, Chats> {
        self.chats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn send(&self, from: UserId, to: UserId, text: &str) -> Result<Message, DomainError> {
        self.lock().send(from, to, text).cloned()
    }

    pub fn history(&self, user: UserId, peer: UserId, limit: usize) -> Vec<Message> {
        self.lock().history(user, peer, limit).to_vec()
    }
}
