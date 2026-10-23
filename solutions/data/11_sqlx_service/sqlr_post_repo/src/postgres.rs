use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use sqlx::PgPool;

use crate::{Cursor, FeedItem, LikeOutcome, LikeRepository, Post, PostRepository, RepoError};

const INSERT_POST: &str = "INSERT INTO posts (author_id, body) VALUES ($1, $2) RETURNING id, author_id, body, likes_count, created_at";
const UPSERT_TAGS: &str = "INSERT INTO tags (name) SELECT * FROM UNNEST($1::text[]) ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id";
const LINK_TAGS: &str = "INSERT INTO post_tags (post_id, tag_id) SELECT $1, * FROM UNNEST($2::bigint[])";
const FEED_PAGE: &str = "SELECT id, author_id, body, likes_count, created_at FROM posts \
    WHERE ($1::timestamptz IS NULL OR (created_at, id) < ($1, $2)) ORDER BY created_at DESC, id DESC LIMIT $3";
const AUTHORS: &str = "SELECT id, username FROM users WHERE id = ANY($1)";
const TAGS_OF: &str = "SELECT pt.post_id, t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = ANY($1) ORDER BY t.name";
const INSERT_LIKE: &str = "INSERT INTO likes (post_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING";
const BUMP_LIKES: &str = "UPDATE posts SET likes_count = likes_count + 1 WHERE id = $1";
const DELETE_LIKE: &str = "DELETE FROM likes WHERE post_id = $1 AND user_id = $2";
const DROP_LIKES: &str = "UPDATE posts SET likes_count = likes_count - 1 WHERE id = $1";

const MAX_ATTEMPTS: u32 = 20;

fn map_error(e: sqlx::Error) -> RepoError {
    if let sqlx::Error::Database(db) = &e {
        match db.code().as_deref() {
            Some("23503") => return RepoError::NotFound,
            Some("23514") => return RepoError::Invalid,
            _ => {}
        }
    }
    RepoError::Db(e)
}

async fn with_retry<T, F, Fut>(mut op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let mut attempt = 1;
    loop {
        match op().await {
            Err(sqlx::Error::Database(e)) if attempt < MAX_ATTEMPTS && matches!(e.code().as_deref(), Some("40001" | "40P01")) => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(2 * u64::from(attempt))).await;
            }
            other => return other,
        }
    }
}

#[derive(Clone)]
pub struct PgPostRepository {
    pool: PgPool,
}

impl PgPostRepository {
    pub fn new(pool: PgPool) -> PgPostRepository {
        PgPostRepository { pool }
    }
}

impl PostRepository for PgPostRepository {
    async fn create_with_tags(&self, author_id: i64, body: String, tags: Vec<String>) -> Result<Post, RepoError> {
        let mut names = tags;
        names.sort();
        names.dedup();
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let post: Post = sqlx::query_as(INSERT_POST)
            .bind(author_id)
            .bind(&body)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_error)?;
        if !names.is_empty() {
            let tag_ids: Vec<i64> = sqlx::query_scalar(UPSERT_TAGS).bind(&names).fetch_all(&mut *tx).await.map_err(map_error)?;
            sqlx::query(LINK_TAGS).bind(post.id).bind(&tag_ids).execute(&mut *tx).await.map_err(map_error)?;
        }
        tx.commit().await.map_err(map_error)?;
        Ok(post)
    }

    async fn feed(&self, limit: i64, after: Option<Cursor>) -> Result<Vec<FeedItem>, RepoError> {
        let (created_at, id) = match after {
            Some(c) => (Some(c.created_at), c.id),
            None => (None, 0),
        };
        let posts: Vec<Post> = sqlx::query_as(FEED_PAGE)
            .bind(created_at)
            .bind(id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;
        if posts.is_empty() {
            return Ok(Vec::new());
        }
        let mut author_ids: Vec<i64> = posts.iter().map(|p| p.author_id).collect();
        author_ids.sort_unstable();
        author_ids.dedup();
        let post_ids: Vec<i64> = posts.iter().map(|p| p.id).collect();
        let authors: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>(AUTHORS)
            .bind(&author_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?
            .into_iter()
            .collect();
        let mut tags: HashMap<i64, Vec<String>> = HashMap::new();
        for (post_id, name) in sqlx::query_as::<_, (i64, String)>(TAGS_OF).bind(&post_ids).fetch_all(&self.pool).await.map_err(map_error)? {
            tags.entry(post_id).or_default().push(name);
        }
        Ok(posts
            .into_iter()
            .map(|post| FeedItem {
                author: authors.get(&post.author_id).cloned().unwrap_or_default(),
                tags: tags.remove(&post.id).unwrap_or_default(),
                post,
            })
            .collect())
    }
}

#[derive(Clone)]
pub struct PgLikeRepository {
    pool: PgPool,
}

impl PgLikeRepository {
    pub fn new(pool: PgPool) -> PgLikeRepository {
        PgLikeRepository { pool }
    }

    async fn like_once(&self, post_id: i64, user_id: i64) -> Result<LikeOutcome, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(INSERT_LIKE).bind(post_id).bind(user_id).execute(&mut *tx).await?.rows_affected();
        if inserted == 0 {
            tx.rollback().await?;
            return Ok(LikeOutcome::AlreadyLiked);
        }
        sqlx::query(BUMP_LIKES).bind(post_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(LikeOutcome::Added)
    }

    async fn unlike_once(&self, post_id: i64, user_id: i64) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let deleted = sqlx::query(DELETE_LIKE).bind(post_id).bind(user_id).execute(&mut *tx).await?.rows_affected();
        if deleted == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query(DROP_LIKES).bind(post_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}

impl LikeRepository for PgLikeRepository {
    async fn like(&self, post_id: i64, user_id: i64) -> Result<LikeOutcome, RepoError> {
        with_retry(|| self.like_once(post_id, user_id)).await.map_err(map_error)
    }

    async fn unlike(&self, post_id: i64, user_id: i64) -> Result<bool, RepoError> {
        with_retry(|| self.unlike_once(post_id, user_id)).await.map_err(map_error)
    }
}
