use contracts::{Cause, UserId};
use journal::JournalError;

use super::{BoxFuture, LifecycleRepository, MemoryUsers, UserEntry};

impl LifecycleRepository for MemoryUsers {
    fn hide(&self, id: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.index(id).is_none() || state.gone(id) {
                return Ok(false);
            }
            self.record(&mut state, UserEntry::Hidden { id: id.0 }, &cause)
                .await?;
            Ok(state.hidden.insert(id))
        })
    }

    fn show(&self, id: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if !state.hidden.contains(&id) {
                return Ok(false);
            }
            self.record(&mut state, UserEntry::Shown { id: id.0 }, &cause)
                .await?;
            Ok(state.hidden.remove(&id))
        })
    }

    fn erase(&self, id: UserId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.index(id).is_none() || state.deleted.contains(&id) {
                return Ok(false);
            }
            self.record(&mut state, UserEntry::Deleted { id: id.0 }, &cause)
                .await?;
            state.erase(id);
            Ok(true)
        })
    }
}
