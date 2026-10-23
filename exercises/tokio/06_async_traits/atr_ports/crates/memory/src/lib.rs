use std::sync::Arc;
use std::time::Duration;

use ports::{Post, PostRepo, RepoResult, User, UserRepo};

pub struct MemUsers {}

impl MemUsers {
    pub fn new() -> MemUsers {
        todo!()
    }
}

impl Default for MemUsers {
    fn default() -> MemUsers {
        MemUsers::new()
    }
}

impl UserRepo for MemUsers {
    async fn create(&self, name: String) -> RepoResult<User> {
        todo!()
    }

    async fn get(&self, id: u32) -> RepoResult<User> {
        todo!()
    }

    async fn count(&self) -> usize {
        todo!()
    }
}

pub struct MemPosts {}

impl MemPosts {
    pub fn new() -> MemPosts {
        todo!()
    }
}

impl Default for MemPosts {
    fn default() -> MemPosts {
        MemPosts::new()
    }
}

impl PostRepo for MemPosts {
    async fn add(&self, author: u32, body: String) -> RepoResult<Post> {
        todo!()
    }

    async fn by_author(&self, author: u32) -> Vec<Post> {
        todo!()
    }
}

pub struct SlowUsers {}

impl SlowUsers {
    pub fn new(inner: Arc<dyn UserRepo>, delay: Duration) -> SlowUsers {
        todo!()
    }
}

pub struct SlowPosts {}

impl SlowPosts {
    pub fn new(inner: Arc<dyn PostRepo>, delay: Duration) -> SlowPosts {
        todo!()
    }
}
