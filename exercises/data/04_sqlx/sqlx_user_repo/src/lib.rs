mod postgres;

use std::future::Future;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub use postgres::PgUserRepository;

pub const SCHEMA: &str = include_str!("../schema.sql");

pub async fn apply_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(SCHEMA).execute(pool).await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewUser {
    pub username: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Post {
    pub id: i64,
    pub author_id: i64,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub enum RepoError {
    UsernameTaken,
    EmailTaken,
    NotFound,
    Invalid,
    Db(sqlx::Error),
}

pub trait UserRepository {
    fn create(&self, new: NewUser) -> impl Future<Output = Result<User, RepoError>> + Send;
    fn find_by_id(&self, id: i64) -> impl Future<Output = Result<Option<User>, RepoError>> + Send;
    fn find_by_username(&self, username: String) -> impl Future<Output = Result<Option<User>, RepoError>> + Send;
    fn users_by_ids(&self, ids: Vec<i64>) -> impl Future<Output = Result<Vec<User>, RepoError>> + Send;
    fn update_bio(&self, id: i64, bio: Option<String>) -> impl Future<Output = Result<(), RepoError>> + Send;
    fn delete(&self, id: i64) -> impl Future<Output = Result<(), RepoError>> + Send;
    fn add_post(&self, author_id: i64, body: String) -> impl Future<Output = Result<Post, RepoError>> + Send;
    fn posts_of(&self, author_id: i64, limit: i64, offset: i64) -> impl Future<Output = Result<Vec<Post>, RepoError>> + Send;
    fn register_with_first_post(&self, new: NewUser, first_post: String) -> impl Future<Output = Result<(User, Post), RepoError>> + Send;
}
