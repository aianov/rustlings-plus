use contracts::{Cause, CommentId, UserId};
use journal::JournalError;

use super::{BoxFuture, MemoryPosts, PostEntry};

pub trait DraftRepository: Send + Sync {
    fn comment_owners(&self, id: CommentId) -> BoxFuture<'_, Option<(UserId, UserId)>>;
    fn remove_comment(
        &self,
        id: CommentId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn save_draft(
        &self,
        owner: UserId,
        text: String,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<u64, JournalError>>;
    fn draft(&self, id: u64) -> BoxFuture<'_, Option<(UserId, String)>>;
    fn delete_draft(&self, id: u64, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>>;
}

impl DraftRepository for MemoryPosts {
    fn comment_owners(&self, id: CommentId) -> BoxFuture<'_, Option<(UserId, UserId)>> {
        Box::pin(async move {
            let state = self.state.lock().await;
            if state.removed.contains(&id) {
                return None;
            }
            let comment = state.comments.iter().find(|comment| comment.id == id)?;
            let post = state.post(comment.post)?;
            Some((comment.author, post.author))
        })
    }

    fn remove_comment(
        &self,
        id: CommentId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if state.removed.contains(&id) || !state.comments.iter().any(|comment| comment.id == id)
            {
                return Ok(false);
            }
            let entry = PostEntry::CommentRemoved { id: id.0 };
            self.record(&mut state, &entry, &cause).await?;
            state.apply(entry);
            Ok(true)
        })
    }

    fn save_draft(
        &self,
        owner: UserId,
        text: String,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<u64, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let id = state.last_draft + 1;
            let entry = PostEntry::DraftSaved {
                id,
                owner: owner.0,
                text,
            };
            self.record(&mut state, &entry, &cause).await?;
            state.apply(entry);
            Ok(id)
        })
    }

    fn draft(&self, id: u64) -> BoxFuture<'_, Option<(UserId, String)>> {
        Box::pin(async move { self.state.lock().await.drafts.get(&id).cloned() })
    }

    fn delete_draft(&self, id: u64, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let mut state = self.state.lock().await;
            if !state.drafts.contains_key(&id) {
                return Ok(false);
            }
            let entry = PostEntry::DraftDeleted { id };
            self.record(&mut state, &entry, &cause).await?;
            state.apply(entry);
            Ok(true)
        })
    }
}
