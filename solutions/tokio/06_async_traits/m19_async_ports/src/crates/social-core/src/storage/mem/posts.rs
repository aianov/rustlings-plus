use std::collections::HashMap;

use crate::domain::{Post, PostId};
use crate::ports::PostStore;

#[derive(Default)]
pub struct MemPostStore {
    posts: Vec<Post>,
    positions: HashMap<PostId, usize>,
    last_id: u64,
}

impl MemPostStore {
    pub fn new() -> MemPostStore {
        MemPostStore::default()
    }
}

impl PostStore for MemPostStore {
    fn next_id(&mut self) -> PostId {
        self.last_id += 1;
        PostId(self.last_id)
    }

    fn insert(&mut self, post: Post) {
        self.positions.insert(post.id(), self.posts.len());
        self.posts.push(post);
    }

    fn get(&self, id: PostId) -> Option<&Post> {
        let position = *self.positions.get(&id)?;
        self.posts.get(position)
    }

    fn get_mut(&mut self, id: PostId) -> Option<&mut Post> {
        let position = *self.positions.get(&id)?;
        self.posts.get_mut(position)
    }

    fn all(&self) -> &[Post] {
        &self.posts
    }
}
