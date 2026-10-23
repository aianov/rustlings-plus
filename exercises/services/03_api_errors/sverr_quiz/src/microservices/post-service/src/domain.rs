use contracts::api::{Post, UserApi};
use contracts::errors::GrpcStatus;
use std::sync::{Arc, Mutex};

pub const MAX_TEXT: usize = 280;

#[derive(Debug, Clone, PartialEq)]
pub enum PostError {
    TextTooLong { len: usize, max: usize },
    UserService { author_id: u64, status: GrpcStatus },
}

pub struct PostService {
    users: Arc<dyn UserApi>,
    posts: Mutex<Vec<Post>>,
}

impl PostService {
    pub fn new(users: Arc<dyn UserApi>) -> PostService {
        PostService { users, posts: Mutex::new(Vec::new()) }
    }

    pub fn create(&self, author_id: u64, text: &str, request_id: &str) -> Result<Post, PostError> {
        let len = text.chars().count();
        if len > MAX_TEXT {
            return Err(PostError::TextTooLong { len, max: MAX_TEXT });
        }
        self.users.get(author_id, request_id).map_err(|status| PostError::UserService { author_id, status })?;
        let mut posts = self.posts.lock().unwrap_or_else(|p| p.into_inner());
        let post = Post { id: posts.len() as u64 + 1, author_id, text: text.to_string() };
        posts.push(post.clone());
        Ok(post)
    }
}
