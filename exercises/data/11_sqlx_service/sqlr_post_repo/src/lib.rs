mod postgres;

use std::future::Future;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub use postgres::{PgLikeRepository, PgPostRepository};

pub const SCHEMA: &str = include_str!("../schema.sql");

pub async fn apply_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA).execute(pool).await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Post {
    pub id: i64,
    pub author_id: i64,
    pub body: String,
    pub likes_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub post: Post,
    pub author: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cursor {
    pub created_at: DateTime<Utc>,
    pub id: i64,
}

#[derive(Debug)]
pub enum RepoError {
    NotFound,
    Invalid,
    Db(sqlx::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LikeOutcome {
    Added,
    AlreadyLiked,
}

pub trait PostRepository {
    fn create_with_tags(&self, author_id: i64, body: String, tags: Vec<String>) -> impl Future<Output = Result<Post, RepoError>> + Send;
    fn feed(&self, limit: i64, after: Option<Cursor>) -> impl Future<Output = Result<Vec<FeedItem>, RepoError>> + Send;
}

pub trait LikeRepository {
    fn like(&self, post_id: i64, user_id: i64) -> impl Future<Output = Result<LikeOutcome, RepoError>> + Send;
    fn unlike(&self, post_id: i64, user_id: i64) -> impl Future<Output = Result<bool, RepoError>> + Send;
}
