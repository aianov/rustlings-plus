use contracts::{Cause, CommentId, UserId};
use service_runtime::StorageError;

use super::pg::{PgPosts, big, small, storage};
use super::{BoxFuture, DraftRepository};

const COMMENT_OWNERS: &str = "SELECT c.author, p.author FROM comments c JOIN posts p ON p.id = c.post WHERE c.id = $1 AND NOT c.removed AND NOT p.deleted";
const REMOVE_COMMENT: &str = "UPDATE comments SET removed = true WHERE id = $1 AND NOT removed";
const INSERT_DRAFT: &str = "INSERT INTO drafts (owner, text) VALUES ($1, $2) RETURNING id";
const SELECT_DRAFT: &str = "SELECT owner, text FROM drafts WHERE id = $1";
const DELETE_DRAFT: &str = "DELETE FROM drafts WHERE id = $1";

impl DraftRepository for PgPosts {
    fn comment_owners(&self, id: CommentId) -> BoxFuture<'_, Option<(UserId, UserId)>> {
        Box::pin(async move {
            let row: Option<(i64, i64)> = sqlx::query_as(COMMENT_OWNERS)
                .bind(big(id.0))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            row.map(|(writer, owner)| (UserId(small(writer)), UserId(small(owner))))
        })
    }

    fn remove_comment(
        &self,
        id: CommentId,
        _cause: &Cause,
    ) -> BoxFuture<'_, Result<bool, StorageError>> {
        Box::pin(async move {
            let done = sqlx::query(REMOVE_COMMENT)
                .bind(big(id.0))
                .execute(&self.pool)
                .await
                .map_err(storage)?;
            Ok(done.rows_affected() == 1)
        })
    }

    fn save_draft(
        &self,
        owner: UserId,
        text: String,
        _cause: &Cause,
    ) -> BoxFuture<'_, Result<u64, StorageError>> {
        Box::pin(async move {
            let id: i64 = sqlx::query_scalar(INSERT_DRAFT)
                .bind(big(owner.0))
                .bind(text)
                .fetch_one(&self.pool)
                .await
                .map_err(storage)?;
            Ok(small(id))
        })
    }

    fn draft(&self, id: u64) -> BoxFuture<'_, Option<(UserId, String)>> {
        Box::pin(async move {
            let row: Option<(i64, String)> = sqlx::query_as(SELECT_DRAFT)
                .bind(big(id))
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
            row.map(|(owner, text)| (UserId(small(owner)), text))
        })
    }

    fn delete_draft(&self, id: u64, _cause: &Cause) -> BoxFuture<'_, Result<bool, StorageError>> {
        Box::pin(async move {
            let done = sqlx::query(DELETE_DRAFT)
                .bind(big(id))
                .execute(&self.pool)
                .await
                .map_err(storage)?;
            Ok(done.rows_affected() == 1)
        })
    }
}
