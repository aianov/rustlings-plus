use super::PostRepository;
use crate::common::Post;
use contracts::{PostId, UserId};
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryPosts {
    posts: Mutex<Vec<Post>>,
}

impl PostRepository for InMemoryPosts {
    fn insert(&self, author: UserId, text: String) -> Post {
        let mut posts = self.posts.lock().unwrap();
        let post = Post { id: PostId(posts.len() as u64 + 1), author, text };
        posts.push(post.clone());
        post
    }

    fn find(&self, id: PostId) -> Option<Post> {
        self.posts.lock().unwrap().iter().find(|p| p.id == id).cloned()
    }
}
