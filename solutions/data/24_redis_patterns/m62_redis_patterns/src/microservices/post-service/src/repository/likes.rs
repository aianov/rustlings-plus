use std::sync::Arc;

use contracts::PostId;
use db_engine::DbError;

use super::BoxFuture;
use super::pg::{COUNT_LIKE, PgPosts, big};

const APPLY_LIKES: &str = "UPDATE posts p SET likes = p.likes + d.delta FROM unnest($1::bigint[], $2::bigint[]) AS d(id, delta) WHERE p.id = d.id";

pub trait PendingLikes: Send + Sync {
    fn add(&self, post: PostId) -> BoxFuture<'_, bool>;
}

impl PgPosts {
    pub fn with_pending(self, pending: Arc<dyn PendingLikes>) -> PgPosts {
        PgPosts {
            pending: Some(pending),
            ..self
        }
    }

    pub(super) async fn deferred(
        &self,
        pending: &Arc<dyn PendingLikes>,
        post: PostId,
    ) -> Result<(), DbError> {
        if pending.add(post).await {
            return Ok(());
        }
        sqlx::query(COUNT_LIKE)
            .bind(big(post.0))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn apply_likes(&self, deltas: &[(PostId, i64)]) -> Result<u64, DbError> {
        let ids: Vec<i64> = deltas.iter().map(|(post, _)| big(post.0)).collect();
        let counts: Vec<i64> = deltas.iter().map(|(_, delta)| *delta).collect();
        let done = sqlx::query(APPLY_LIKES)
            .bind(ids)
            .bind(counts)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected())
    }
}
