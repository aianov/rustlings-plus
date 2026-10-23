mod directory;
mod error;
mod hub;

use std::sync::Arc;

use contracts::UserId;
use tokio::sync::mpsc;

use crate::common::{MAX_MESSAGE_LEN, Message};
use crate::repository::ChatRepository;

pub use directory::{DirectoryError, UserDirectory};
pub use error::ChatError;
pub use hub::{BACKLOG, Hub};

pub struct ChatService {
    chats: Arc<dyn ChatRepository>,
    hub: Arc<Hub>,
}

impl ChatService {
    pub fn new(chats: Arc<dyn ChatRepository>, hub: Arc<Hub>) -> ChatService {
        ChatService { chats, hub }
    }

    async fn known(&self, users: &dyn UserDirectory, peer: UserId) -> Result<(), ChatError> {
        if users.exists(peer).await? {
            Ok(())
        } else {
            Err(ChatError::UnknownPeer(peer))
        }
    }

    pub async fn send(
        &self,
        users: &dyn UserDirectory,
        from: UserId,
        to: UserId,
        text: &str,
    ) -> Result<Message, ChatError> {
        if from == to {
            return Err(ChatError::SelfMessage);
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(ChatError::EmptyMessage);
        }
        if text.chars().count() > MAX_MESSAGE_LEN {
            return Err(ChatError::MessageTooLong);
        }
        self.known(users, to).await?;
        let message = Message {
            from,
            to,
            text: text.to_string(),
        };
        let stored = self.chats.append(message).await?;
        self.hub.deliver(&stored);
        Ok(stored)
    }

    pub async fn history(
        &self,
        users: &dyn UserDirectory,
        user: UserId,
        peer: UserId,
        limit: usize,
    ) -> Result<Vec<Message>, ChatError> {
        self.known(users, peer).await?;
        Ok(self.chats.history(user, peer, limit).await)
    }

    pub fn subscribe(&self, user: UserId) -> Result<mpsc::Receiver<Message>, ChatError> {
        self.hub.subscribe(user)
    }
}
