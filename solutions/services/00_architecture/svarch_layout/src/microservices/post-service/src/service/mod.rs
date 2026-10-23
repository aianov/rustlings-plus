use crate::common::Post;
use crate::repository::PostRepository;
use contracts::{PostId, UserId};
use std::sync::Arc;

const MAX_POST_CHARS: usize = 280;

#[derive(Debug, PartialEq)]
pub enum PostError {
    Empty,
    TooLong,
}

pub struct PostService {
    repo: Arc<dyn PostRepository>,
}

impl PostService {
    pub fn new(repo: Arc<dyn PostRepository>) -> Self {
        Self { repo }
    }

    pub fn create(&self, author: UserId, text: &str) -> Result<Post, PostError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(PostError::Empty);
        }
        if text.chars().count() > MAX_POST_CHARS {
            return Err(PostError::TooLong);
        }
        Ok(self.repo.insert(author, text.to_string()))
    }

    pub fn get(&self, id: PostId) -> Option<Post> {
        self.repo.find(id)
    }
}
