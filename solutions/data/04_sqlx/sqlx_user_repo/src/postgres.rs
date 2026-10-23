use sqlx::PgPool;

use crate::{NewUser, Post, RepoError, User, UserRepository};

const INSERT_USER: &str = "INSERT INTO users (username, email) VALUES ($1, $2) RETURNING id, username, email, bio, created_at";
const SELECT_USER_BY_ID: &str = "SELECT id, username, email, bio, created_at FROM users WHERE id = $1";
const SELECT_USER_BY_USERNAME: &str = "SELECT id, username, email, bio, created_at FROM users WHERE username = $1";
const SELECT_USERS_BY_IDS: &str =
    "SELECT id, username, email, bio, created_at FROM users WHERE id = ANY($1) ORDER BY array_position($1, id)";
const UPDATE_BIO: &str = "UPDATE users SET bio = $2 WHERE id = $1";
const DELETE_USER: &str = "DELETE FROM users WHERE id = $1";
const INSERT_POST: &str = "INSERT INTO posts (author_id, body) VALUES ($1, $2) RETURNING id, author_id, body, created_at";
const SELECT_POSTS_OF: &str =
    "SELECT id, author_id, body, created_at FROM posts WHERE author_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3";

#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        PgUserRepository { pool }
    }
}

fn map_error(e: sqlx::Error) -> RepoError {
    if let sqlx::Error::Database(db) = &e {
        match (db.code().as_deref(), db.constraint()) {
            (Some("23505"), Some("users_username_uniq")) => return RepoError::UsernameTaken,
            (Some("23505"), Some("users_email_uniq")) => return RepoError::EmailTaken,
            (Some("23503"), _) => return RepoError::NotFound,
            (Some("23514"), _) => return RepoError::Invalid,
            _ => {}
        }
    }
    RepoError::Db(e)
}

impl UserRepository for PgUserRepository {
    async fn create(&self, new: NewUser) -> Result<User, RepoError> {
        sqlx::query_as::<_, User>(INSERT_USER)
            .bind(new.username)
            .bind(new.email)
            .fetch_one(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn find_by_id(&self, id: i64) -> Result<Option<User>, RepoError> {
        sqlx::query_as::<_, User>(SELECT_USER_BY_ID)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn find_by_username(&self, username: String) -> Result<Option<User>, RepoError> {
        sqlx::query_as::<_, User>(SELECT_USER_BY_USERNAME)
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn users_by_ids(&self, ids: Vec<i64>) -> Result<Vec<User>, RepoError> {
        sqlx::query_as::<_, User>(SELECT_USERS_BY_IDS)
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn update_bio(&self, id: i64, bio: Option<String>) -> Result<(), RepoError> {
        let done = sqlx::query(UPDATE_BIO).bind(id).bind(bio).execute(&self.pool).await.map_err(map_error)?;
        if done.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }
        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<(), RepoError> {
        let done = sqlx::query(DELETE_USER).bind(id).execute(&self.pool).await.map_err(map_error)?;
        if done.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }
        Ok(())
    }

    async fn add_post(&self, author_id: i64, body: String) -> Result<Post, RepoError> {
        sqlx::query_as::<_, Post>(INSERT_POST)
            .bind(author_id)
            .bind(body)
            .fetch_one(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn posts_of(&self, author_id: i64, limit: i64, offset: i64) -> Result<Vec<Post>, RepoError> {
        sqlx::query_as::<_, Post>(SELECT_POSTS_OF)
            .bind(author_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)
    }

    async fn register_with_first_post(&self, new: NewUser, first_post: String) -> Result<(User, Post), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let user = sqlx::query_as::<_, User>(INSERT_USER)
            .bind(new.username)
            .bind(new.email)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_error)?;
        let post = sqlx::query_as::<_, Post>(INSERT_POST)
            .bind(user.id)
            .bind(first_post)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok((user, post))
    }
}
