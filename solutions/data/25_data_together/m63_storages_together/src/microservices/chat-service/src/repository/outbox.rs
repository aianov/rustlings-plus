use std::sync::Arc;

use contracts::OutboxRow;
use service_runtime::StorageError;
use tokio::sync::Notify;

use super::{BoxFuture, MemoryChats, OutboxRepository};

impl OutboxRepository for MemoryChats {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>> {
        Box::pin(async move { self.state.lock().await.outbox.unpublished(limit) })
    }

    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if through <= state.outbox.published() {
                return Ok(());
            }
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
