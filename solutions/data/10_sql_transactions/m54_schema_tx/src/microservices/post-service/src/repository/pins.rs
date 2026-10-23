use contracts::{CommentId, PostId, UserId};
use journal::JournalError;

use super::BoxFuture;
use super::pg::{PgPosts, big, small, storage};

const ENSURE_AUTHOR: &str = "INSERT INTO post_authors (author) VALUES ($1) ON CONFLICT DO NOTHING";
const LOCK_AUTHOR: &str = "SELECT author FROM post_authors WHERE author = $1 FOR UPDATE";
const OWNER: &str = "SELECT author, pinned FROM posts WHERE id = $1 AND NOT deleted";
const COUNT_PINNED: &str =
    "SELECT count(*) FROM posts WHERE author = $1 AND pinned AND NOT deleted";
const SET_PIN: &str = "UPDATE posts SET pinned = $2 WHERE id = $1";
const PINNED: &str =
    "SELECT id FROM posts WHERE author = $1 AND pinned AND NOT deleted ORDER BY id DESC";
const LIKE_COMMENT: &str = "INSERT INTO comment_likes (comment, user_id, at) SELECT $1, $2, $3 WHERE EXISTS (SELECT 1 FROM comments WHERE id = $1 AND NOT removed) ON CONFLICT DO NOTHING";
const COUNT_COMMENT_LIKE: &str = "UPDATE comments SET likes = likes + 1 WHERE id = $1";
const COMMENT_ALIVE: &str = "SELECT likes FROM comments WHERE id = $1 AND NOT removed";

pub const MAX_PINS: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pinned {
    Changed(bool),
    Missing,
    NotAuthor,
    Full,
}

pub trait PinQueries: Send + Sync {
    fn pin(
        &self,
        author: UserId,
        post: PostId,
        on: bool,
    ) -> BoxFuture<'_, Result<Pinned, JournalError>>;
    fn pinned(&self, author: UserId) -> BoxFuture<'_, Vec<PostId>>;
    fn like_comment(
        &self,
        user: UserId,
        comment: CommentId,
        at: u64,
    ) -> BoxFuture<'_, Result<Option<(bool, u64)>, JournalError>>;
}

impl PinQueries for PgPosts {
    fn pin(
        &self,
        author: UserId,
        post: PostId,
        on: bool,
    ) -> BoxFuture<'_, Result<Pinned, JournalError>> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(storage)?;
            sqlx::query(ENSURE_AUTHOR)
                .bind(big(author.0))
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            sqlx::query(LOCK_AUTHOR)
                .bind(big(author.0))
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            let owner: Option<(i64, bool)> = sqlx::query_as(OWNER)
                .bind(big(post.0))
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
            let Some((owner, pinned)) = owner else {
                return Ok(Pinned::Missing);
            };
            if small(owner) != author.0 {
                return Ok(Pinned::NotAuthor);
            }
            if pinned == on {
                return Ok(Pinned::Changed(false));
            }
            if on {
                let count: i64 = sqlx::query_scalar(COUNT_PINNED)
                    .bind(big(author.0))
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(storage)?;
                if count >= MAX_PINS {
                    return Ok(Pinned::Full);
                }
            }
            sqlx::query(SET_PIN)
                .bind(big(post.0))
                .bind(on)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(Pinned::Changed(true))
        })
    }

    fn pinned(&self, author: UserId) -> BoxFuture<'_, Vec<PostId>> {
        Box::pin(async move {
            let rows: Vec<i64> = sqlx::query_scalar(PINNED)
                .bind(big(author.0))
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
            rows.into_iter().map(|id| PostId(small(id))).collect()
        })
    }

    fn like_comment(
        &self,
        user: UserId,
        comment: CommentId,
        at: u64,
    ) -> BoxFuture<'_, Result<Option<(bool, u64)>, JournalError>> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(storage)?;
            let added = sqlx::query(LIKE_COMMENT)
                .bind(big(comment.0))
                .bind(big(user.0))
                .bind(big(at))
                .execute(&mut *tx)
                .await
                .map_err(storage)?
                .rows_affected()
                == 1;
            if added {
                sqlx::query(COUNT_COMMENT_LIKE)
                    .bind(big(comment.0))
                    .execute(&mut *tx)
                    .await
                    .map_err(storage)?;
            }
            let likes: Option<i32> = sqlx::query_scalar(COMMENT_ALIVE)
                .bind(big(comment.0))
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(likes.map(|likes| (added, u64::try_from(likes).unwrap_or(0))))
        })
    }
}
