use std::sync::Arc;

use contracts::{Cause, CommentId, Fact, OutboxRow, PostDeleted, PostEdited, PostId, UserId};
use journal::JournalError;
use tokio::sync::Notify;

use super::pg::{
    DELETE_POST, EDIT_POST, LIKES_SINCE, PgPosts, PostRow, SELECT_BY_AUTHORS, big, small, storage,
};
use super::{BoxFuture, OutboxRepository, PostRepository};
use crate::common::{CommentText, Draft, Liked, Post, PostText};

impl PostRepository for PgPosts {
    fn insert(
        &self,
        author: UserId,
        author_name: String,
        draft: Draft,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Post, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            self.create(author, author_name, draft, &cause)
                .await
                .map_err(storage)
        })
    }

    fn get(&self, id: PostId) -> BoxFuture<'_, Option<Post>> {
        Box::pin(async move { self.post(id).await.ok().flatten() })
    }

    fn like_if_absent(
        &self,
        user: UserId,
        post: PostId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<Liked>, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move { self.like(user, post, &cause).await.map_err(storage) })
    }

    fn edit(
        &self,
        post: PostId,
        text: PostText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<Post>, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let fact = Fact::PostEdited(PostEdited {
                post_id: post,
                text: text.as_ref().to_string(),
            });
            let changed = self
                .change(EDIT_POST, post, Some(text.as_ref()), fact, &cause)
                .await
                .map_err(storage)?;
            if !changed {
                return Ok(None);
            }
            self.post(post).await.map_err(storage)
        })
    }

    fn delete(&self, post: PostId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            let fact = Fact::PostDeleted(PostDeleted { post_id: post });
            self.change(DELETE_POST, post, None, fact, &cause)
                .await
                .map_err(storage)
        })
    }

    fn by_authors(
        &self,
        authors: Vec<UserId>,
        skip: usize,
        take: usize,
    ) -> BoxFuture<'_, Vec<Post>> {
        Box::pin(async move {
            let ids: Vec<i64> = authors.iter().map(|author| big(author.0)).collect();
            let rows: Vec<PostRow> = sqlx::query_as(SELECT_BY_AUTHORS)
                .bind(ids)
                .bind(i64::try_from(skip).unwrap_or(i64::MAX))
                .bind(i64::try_from(take).unwrap_or(i64::MAX))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            self.assemble(rows).await.unwrap_or_default()
        })
    }

    fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: CommentText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<CommentId>, JournalError>> {
        let cause = cause.clone();
        Box::pin(async move {
            self.reply(author, post, text.as_ref(), &cause)
                .await
                .map_err(storage)
        })
    }

    fn likes_since(&self, since: u64) -> BoxFuture<'_, Vec<(PostId, usize)>> {
        Box::pin(async move {
            let rows: Vec<(i64, i64)> = sqlx::query_as(LIKES_SINCE)
                .bind(big(since))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter()
                .map(|(post, count)| (PostId(small(post)), usize::try_from(count).unwrap_or(0)))
                .collect()
        })
    }

    fn flush(&self) -> BoxFuture<'_, Result<usize, JournalError>> {
        Box::pin(async { Ok(0) })
    }
}

impl OutboxRepository for PgPosts {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>> {
        Box::pin(async move {
            db_engine::unpublished(&self.pool, limit)
                .await
                .unwrap_or_default()
        })
    }

    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>> {
        Box::pin(async move {
            db_engine::mark_published(&self.pool, through)
                .await
                .map_err(storage)
        })
    }

    fn waiting(&self) -> BoxFuture<'_, usize> {
        Box::pin(async move { db_engine::waiting(&self.pool).await.unwrap_or(0) })
    }

    fn signal(&self) -> Arc<Notify> {
        Arc::clone(&self.signal)
    }
}
