use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use ports::{Post, PostRepo, RepoError, RepoResult, User, UserRepo};
use tokio::time::sleep;

#[derive(Default)]
pub struct MemUsers {
    users: Mutex<Vec<User>>,
}

impl MemUsers {
    pub fn new() -> MemUsers {
        MemUsers::default()
    }
}

impl UserRepo for MemUsers {
    fn create(&self, name: String) -> BoxFuture<'_, RepoResult<User>> {
        async move {
            let mut users = self.users.lock().unwrap();
            if users.iter().any(|user| user.name == name) {
                return Err(RepoError::Conflict);
            }
            let user = User { id: users.len() as u32 + 1, name };
            users.push(user.clone());
            Ok(user)
        }
        .boxed()
    }

    fn get(&self, id: u32) -> BoxFuture<'_, RepoResult<User>> {
        async move { self.users.lock().unwrap().iter().find(|user| user.id == id).cloned().ok_or(RepoError::NotFound) }.boxed()
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        async move { self.users.lock().unwrap().len() }.boxed()
    }
}

#[derive(Default)]
pub struct MemPosts {
    posts: Mutex<Vec<Post>>,
}

impl MemPosts {
    pub fn new() -> MemPosts {
        MemPosts::default()
    }
}

impl PostRepo for MemPosts {
    fn add(&self, author: u32, body: String) -> BoxFuture<'_, RepoResult<Post>> {
        async move {
            let mut posts = self.posts.lock().unwrap();
            let post = Post { id: posts.len() as u32 + 1, author, body };
            posts.push(post.clone());
            Ok(post)
        }
        .boxed()
    }

    fn by_author(&self, author: u32) -> BoxFuture<'_, Vec<Post>> {
        async move { self.posts.lock().unwrap().iter().filter(|post| post.author == author).cloned().collect() }.boxed()
    }
}

pub struct SlowUsers {
    inner: Arc<dyn UserRepo>,
    delay: Duration,
}

impl SlowUsers {
    pub fn new(inner: Arc<dyn UserRepo>, delay: Duration) -> SlowUsers {
        SlowUsers { inner, delay }
    }
}

impl UserRepo for SlowUsers {
    fn create(&self, name: String) -> BoxFuture<'_, RepoResult<User>> {
        async move {
            sleep(self.delay).await;
            self.inner.create(name).await
        }
        .boxed()
    }

    fn get(&self, id: u32) -> BoxFuture<'_, RepoResult<User>> {
        async move {
            sleep(self.delay).await;
            self.inner.get(id).await
        }
        .boxed()
    }

    fn count(&self) -> BoxFuture<'_, usize> {
        async move {
            sleep(self.delay).await;
            self.inner.count().await
        }
        .boxed()
    }
}

pub struct SlowPosts {
    inner: Arc<dyn PostRepo>,
    delay: Duration,
}

impl SlowPosts {
    pub fn new(inner: Arc<dyn PostRepo>, delay: Duration) -> SlowPosts {
        SlowPosts { inner, delay }
    }
}

impl PostRepo for SlowPosts {
    fn add(&self, author: u32, body: String) -> BoxFuture<'_, RepoResult<Post>> {
        async move {
            sleep(self.delay).await;
            self.inner.add(author, body).await
        }
        .boxed()
    }

    fn by_author(&self, author: u32) -> BoxFuture<'_, Vec<Post>> {
        async move {
            sleep(self.delay).await;
            self.inner.by_author(author).await
        }
        .boxed()
    }
}
