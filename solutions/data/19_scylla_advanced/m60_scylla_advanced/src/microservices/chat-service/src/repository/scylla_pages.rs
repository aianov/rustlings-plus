use std::sync::Arc;

use contracts::{OutboxRow, UserId};
use futures::StreamExt;
use journal::JournalError;
use store_engine::{Cursor, StoreError};
use tokio::sync::{Notify, mpsc};

use super::scylla::{
    BUCKETS_BACK, MessageRow, ScyllaChats, big, bucket_of, chat_of, message_of, small,
};
use super::{BoxFuture, ChatPages, ChatSummary, OutboxRepository, Page};
use crate::common::{Message, now_micros};

impl ChatPages for ScyllaChats {
    fn page(
        &self,
        user: UserId,
        peer: UserId,
        cursor: Option<String>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Page, StoreError>> {
        Box::pin(async move {
            let cursor = match cursor.filter(|cursor| !cursor.is_empty()) {
                Some(raw) => Some(Cursor::decode(&raw)?),
                None => None,
            };
            self.page_from(user, peer, cursor, limit).await
        })
    }

    fn chats(
        &self,
        user: UserId,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<ChatSummary>, StoreError>> {
        Box::pin(async move {
            let rows = self
                .session
                .execute(
                    &self.st.chats,
                    (big(user.0), i32::try_from(limit).unwrap_or(i32::MAX)),
                )
                .await?
                .rows_typed_or_empty::<(i64, i64, String)>();
            let mut chats = Vec::new();
            for row in rows {
                let (peer, last_at, preview) =
                    row.map_err(|error| StoreError::Other(error.to_string()))?;
                let peer = UserId(small(peer));
                chats.push(ChatSummary {
                    peer,
                    last_at: small(last_at),
                    preview,
                    unread: self.unread_of(user, peer).await?,
                });
            }
            Ok(chats)
        })
    }

    fn export(
        &self,
        user: UserId,
        peer: UserId,
        sink: mpsc::Sender<Message>,
    ) -> BoxFuture<'_, Result<usize, StoreError>> {
        Box::pin(async move {
            let chat = chat_of(user, peer);
            let newest = bucket_of(now_micros());
            let mut sent = 0;
            for bucket in (newest - BUCKETS_BACK + 1..=newest).rev() {
                let rows = self
                    .session
                    .execute_iter(self.st.history.clone(), (chat.as_str(), bucket))
                    .await?;
                let mut rows = rows.into_typed::<MessageRow>();
                while let Some(row) = rows.next().await {
                    let row = row.map_err(|error| StoreError::Other(error.to_string()))?;
                    if sink.send(message_of(row)).await.is_err() {
                        return Ok(sent);
                    }
                    sent += 1;
                }
            }
            Ok(sent)
        })
    }
}

impl OutboxRepository for ScyllaChats {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>> {
        Box::pin(async move { self.outbox.lock().await.unpublished(limit) })
    }

    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>> {
        Box::pin(async move {
            self.outbox.lock().await.published_through(through);
            Ok(())
        })
    }

    fn waiting(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { self.outbox.lock().await.pending() })
    }

    fn signal(&self) -> Arc<Notify> {
        Arc::clone(&self.signal)
    }
}
