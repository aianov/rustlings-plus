mod directory;
mod error;
mod pages;
mod typing;

use std::sync::Arc;
use std::time::Instant;

use contracts::{Cause, Origin, TypingNote, UserId};

use crate::common::{Message, MessageText, now_micros};
use crate::repository::{ChatPages, ChatRepository};

pub use directory::{DirectoryError, UserDirectory};
pub use error::ChatError;
pub use typing::{Offline, Pace, TYPING_PACE, Typist, Undelivered};

pub struct ChatService {
    chats: Arc<dyn ChatRepository>,
    pages: Option<Arc<dyn ChatPages>>,
    typist: Arc<dyn Typist>,
    pace: Pace,
}

impl ChatService {
    pub fn new(chats: Arc<dyn ChatRepository>, typist: Arc<dyn Typist>) -> ChatService {
        ChatService {
            chats,
            pages: None,
            typist,
            pace: Pace::default(),
        }
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
        text: MessageText,
        origin: &Origin,
    ) -> Result<Message, ChatError> {
        if from == to {
            return Err(ChatError::SelfMessage);
        }
        self.known(users, to).await?;
        let message = Message {
            from,
            to,
            text: text.as_ref().to_string(),
        };
        let cause = Cause::new(origin, now_micros());
        Ok(self.chats.append(message, &cause).await?)
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

    pub async fn typing(
        &self,
        users: &dyn UserDirectory,
        from: UserId,
        to: UserId,
        origin: &Origin,
    ) -> Result<bool, ChatError> {
        if from == to {
            return Err(ChatError::SelfMessage);
        }
        self.known(users, to).await?;
        if !self.pace.allow(from, to, Instant::now()) {
            return Ok(false);
        }
        let note = TypingNote { from, to };
        self.typist
            .typing(note, origin.clone())
            .await
            .map_err(|Undelivered| ChatError::Undelivered)?;
        Ok(true)
    }
}
