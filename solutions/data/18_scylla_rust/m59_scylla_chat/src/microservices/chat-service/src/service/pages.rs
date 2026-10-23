use std::sync::Arc;

use contracts::UserId;
use tokio::sync::mpsc;

use super::directory::UserDirectory;
use super::{ChatError, ChatService};
use crate::common::Message;
use crate::repository::{ChatPages, ChatSummary};

pub const CHATS_MAX: usize = 50;

pub const EXPORT_BUFFER: usize = 64;

impl ChatService {
    pub fn with_pages(self, pages: Arc<dyn ChatPages>) -> ChatService {
        ChatService {
            pages: Some(pages),
            ..self
        }
    }

    pub fn paged(&self) -> bool {
        self.pages.is_some()
    }

    fn pages(&self) -> Result<Arc<dyn ChatPages>, ChatError> {
        self.pages.clone().ok_or(ChatError::NoPages)
    }

    pub async fn page(
        &self,
        users: &dyn UserDirectory,
        user: UserId,
        peer: UserId,
        cursor: String,
        limit: usize,
    ) -> Result<(Vec<Message>, Option<String>), ChatError> {
        self.known(users, peer).await?;
        let (mut messages, next) = self.pages()?.page(user, peer, Some(cursor), limit).await?;
        messages.reverse();
        Ok((messages, next))
    }

    pub async fn chats(&self, user: UserId, limit: usize) -> Result<Vec<ChatSummary>, ChatError> {
        let limit = match limit {
            0 => 20,
            limit => limit.min(CHATS_MAX),
        };
        Ok(self.pages()?.chats(user, limit).await?)
    }

    pub async fn export(
        &self,
        users: &dyn UserDirectory,
        user: UserId,
        peer: UserId,
    ) -> Result<mpsc::Receiver<Message>, ChatError> {
        self.known(users, peer).await?;
        let pages = self.pages()?;
        let (tx, rx) = mpsc::channel(EXPORT_BUFFER);
        tokio::spawn(async move {
            if let Err(error) = pages.export(user, peer, tx).await {
                tracing::warn!("chat-service: выгрузка переписки оборвалась: {error}");
            }
        });
        Ok(rx)
    }
}
