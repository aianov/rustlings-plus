use sqlx::PgPool;

use crate::{NewUser, Post, RepoError, User, UserRepository};

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
    todo!()
}

impl UserRepository for PgUserRepository {
    async fn create(&self, new: NewUser) -> Result<User, RepoError> {
        todo!()
    }

    async fn find_by_id(&self, id: i64) -> Result<Option<User>, RepoError> {
        todo!()
    }

    async fn find_by_username(&self, username: String) -> Result<Option<User>, RepoError> {
        todo!()
    }

    async fn users_by_ids(&self, ids: Vec<i64>) -> Result<Vec<User>, RepoError> {
        todo!()
    }

    async fn update_bio(&self, id: i64, bio: Option<String>) -> Result<(), RepoError> {
        todo!()
    }

    async fn delete(&self, id: i64) -> Result<(), RepoError> {
        todo!()
    }

    async fn add_post(&self, author_id: i64, body: String) -> Result<Post, RepoError> {
        todo!()
    }

    async fn posts_of(&self, author_id: i64, limit: i64, offset: i64) -> Result<Vec<Post>, RepoError> {
        todo!()
    }

    async fn register_with_first_post(&self, new: NewUser, first_post: String) -> Result<(User, Post), RepoError> {
        todo!()
    }
}
