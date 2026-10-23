use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const MAX_TEXT: usize = 280;

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author_id: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PostError {
    NotFound(u64),
    AuthorNotFound(u64),
    TextTooLong { len: usize, max: usize },
    Unavailable,
    Internal(String),
}

pub enum DirectoryAnswer {
    Exists,
    Missing,
    Down,
    Broken(String),
}

pub trait UserDirectory: Send + Sync {
    fn lookup(&self, user_id: u64) -> DirectoryAnswer;
}

pub struct PostService {
    directory: Arc<dyn UserDirectory>,
    posts: Mutex<HashMap<u64, Post>>,
}

impl PostService {
    pub fn new(directory: Arc<dyn UserDirectory>, seed: Vec<Post>) -> PostService {
        PostService { directory, posts: Mutex::new(seed.into_iter().map(|p| (p.id, p)).collect()) }
    }

    pub fn get(&self, id: u64) -> Result<Post, PostError> {
        self.posts.lock().expect("хранилище").get(&id).cloned().ok_or(PostError::NotFound(id))
    }

    pub fn create(&self, author_id: u64, text: &str) -> Result<Post, PostError> {
        let len = text.chars().count();
        if len > MAX_TEXT {
            return Err(PostError::TextTooLong { len, max: MAX_TEXT });
        }
        match self.directory.lookup(author_id) {
            DirectoryAnswer::Exists => {}
            DirectoryAnswer::Missing => return Err(PostError::AuthorNotFound(author_id)),
            DirectoryAnswer::Down => return Err(PostError::Unavailable),
            DirectoryAnswer::Broken(text) => return Err(PostError::Internal(text)),
        }
        let mut posts = self.posts.lock().expect("хранилище");
        let id = posts.len() as u64 + 1;
        let post = Post { id, author_id, text: text.to_string() };
        posts.insert(id, post.clone());
        Ok(post)
    }
}
