use std::sync::Arc;

use contracts::OutboxRow;
use journal::JournalError;
use tokio::sync::Notify;

use super::{BoxFuture, MemoryPosts, OutboxRepository, PostEntry, PostLine};

impl OutboxRepository for MemoryPosts {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let _ = self.flush_pending(&mut state).await;
            state.outbox.unpublished(limit)
        })
    }

    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if through <= state.outbox.published() {
                return Ok(());
            }
            let line = PostLine {
                entry: PostEntry::Published { through },
                outbox: None,
            };
            self.journal.append(&line).await?;
            state.outbox.published_through(through);
            Ok(())
        })
    }

    fn waiting(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { self.state.lock().await.outbox.pending() })
    }

    fn signal(&self) -> Arc<Notify> {
        Arc::clone(&self.signal)
    }
}
