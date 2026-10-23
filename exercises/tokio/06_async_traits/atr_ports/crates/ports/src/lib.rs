use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u32,
    pub author: u32,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RepoError {
    NotFound,
    Conflict,
}

pub type RepoResult<T> = Result<T, RepoError>;

pub trait UserRepo: Send + Sync {
    async fn create(&self, name: String) -> RepoResult<User>;
    async fn get(&self, id: u32) -> RepoResult<User>;
    async fn count(&self) -> usize;
}

pub trait PostRepo: Send + Sync {
    async fn add(&self, author: u32, body: String) -> RepoResult<Post>;
    async fn by_author(&self, author: u32) -> Vec<Post>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub user: User,
    pub posts: Vec<Post>,
}

pub struct Service {
    users: Arc<dyn UserRepo>,
    posts: Arc<dyn PostRepo>,
}

impl Service {
    pub fn new(users: Arc<dyn UserRepo>, posts: Arc<dyn PostRepo>) -> Service {
        Service { users, posts }
    }

    pub async fn register(&self, name: &str) -> RepoResult<User> {
        todo!()
    }

    pub async fn publish(&self, author: u32, body: &str) -> RepoResult<Post> {
        todo!()
    }

    pub async fn profile(&self, id: u32) -> RepoResult<Profile> {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Ok(String),
    Error(String),
    UnknownCommand,
}

pub struct Commands {}

impl Commands {
    pub fn new(service: Arc<Service>) -> Commands {
        todo!()
    }

    pub fn register<F, Fut>(&mut self, name: &str, handler: F) {
        todo!()
    }

    pub async fn dispatch(&self, name: &str, args: Vec<String>) -> Response {
        todo!()
    }
}
