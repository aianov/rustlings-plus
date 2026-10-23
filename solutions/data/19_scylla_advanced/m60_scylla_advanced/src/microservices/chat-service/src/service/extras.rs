use std::sync::Arc;

use contracts::UserId;

use super::directory::UserDirectory;
use super::{ChatError, ChatService};
use crate::common::{Message, MessageText};
use crate::repository::{ChatExtras, Edited};

pub const MAX_TTL: u32 = 7 * 24 * 60 * 60;

impl ChatService {
    pub fn with_extras(self, extras: Arc<dyn ChatExtras>) -> ChatService {
        ChatService {
            extras: Some(extras),
            ..self
        }
    }

    fn extras(&self) -> Result<Arc<dyn ChatExtras>, ChatError> {
        self.extras.clone().ok_or(ChatError::NoPages)
    }

    pub async fn send_for(
        &self,
        users: &dyn UserDirectory,
        from: UserId,
        to: UserId,
        text: MessageText,
        ttl: u32,
    ) -> Result<Message, ChatError> {
        if from == to {
            return Err(ChatError::SelfMessage);
        }
        if ttl == 0 || ttl > MAX_TTL {
            return Err(ChatError::BadTtl(ttl));
        }
        self.known(users, to).await?;
        let message = Message {
            from,
            to,
            text: text.as_ref().to_string(),
            id: String::new(),
            edited: false,
        };
        Ok(self.extras()?.send_for(message, ttl).await?)
    }

    pub async fn edit(&self, user: UserId, id: String, text: MessageText) -> Result<(), ChatError> {
        let edited = self
            .extras()?
            .edit(user, id.clone(), text.as_ref().to_string())
            .await?;
        match edited {
            Edited::Changed => Ok(()),
            Edited::Missing => Err(ChatError::UnknownMessage(id)),
            Edited::NotAuthor => Err(ChatError::NotAuthor(id)),
        }
    }

    pub async fn mark_read(&self, user: UserId, peer: UserId) -> Result<(), ChatError> {
        Ok(self.extras()?.mark_read(user, peer).await?)
    }
}
