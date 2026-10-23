use crate::common::Post;
use crate::repository::{PostRepository, RepoError};
use contracts::{PostId, UserId};
use futures::future::BoxFuture;
use std::sync::Arc;

const MAX_POST_CHARS: usize = 280;

#[derive(Debug, PartialEq)]
pub enum DirectoryError {
    Unavailable,
}

pub trait UserDirectory: Send + Sync {
    fn exists(&self, id: UserId) -> BoxFuture<'_, Result<bool, DirectoryError>>;
}

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    TooLong,
    AuthorNotFound,
    AuthorCheckUnavailable,
    Duplicate,
    Storage,
}

pub struct PostService {
    users: Arc<dyn UserDirectory>,
    posts: Arc<dyn PostRepository>,
}

impl PostService {
    pub fn new(users: Arc<dyn UserDirectory>, posts: Arc<dyn PostRepository>) -> Self {
        Self { users, posts }
    }

    pub async fn create(&self, author: UserId, text: &str) -> Result<Post, PostError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(PostError::Empty);
        }
        if text.chars().count() > MAX_POST_CHARS {
            return Err(PostError::TooLong);
        }
        match self.users.exists(author).await {
            Ok(true) => {}
            Ok(false) => return Err(PostError::AuthorNotFound),
            Err(DirectoryError::Unavailable) => return Err(PostError::AuthorCheckUnavailable),
        }
        self.posts
            .create_if_absent(author, text.to_string())
            .await
            .map_err(|error| match error {
                RepoError::Conflict => PostError::Duplicate,
                RepoError::Unavailable => PostError::Storage,
            })
    }

    pub async fn get(&self, id: PostId) -> Option<Post> {
        self.posts.find(id).await
    }

    pub async fn by_author(&self, author: UserId) -> Vec<Post> {
        self.posts.by_author(author).await
    }
}
