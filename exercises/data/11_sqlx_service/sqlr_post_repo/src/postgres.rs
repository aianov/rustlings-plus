use sqlx::PgPool;

use crate::{Cursor, FeedItem, LikeOutcome, LikeRepository, Post, PostRepository, RepoError};

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
        let _ = (&self.pool, author_id, body, tags);
        todo!()
    }

    async fn feed(&self, limit: i64, after: Option<Cursor>) -> Result<Vec<FeedItem>, RepoError> {
        let _ = (&self.pool, limit, after);
        todo!()
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
}

impl LikeRepository for PgLikeRepository {
    async fn like(&self, post_id: i64, user_id: i64) -> Result<LikeOutcome, RepoError> {
        let _ = (&self.pool, post_id, user_id);
        todo!()
    }

    async fn unlike(&self, post_id: i64, user_id: i64) -> Result<bool, RepoError> {
        let _ = (&self.pool, post_id, user_id);
        todo!()
    }
}
