use crate::errors::GrpcStatus;

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: u64,
    pub author_id: u64,
    pub text: String,
}

pub trait UserApi: Send + Sync {
    fn get(&self, id: u64, request_id: &str) -> Result<User, GrpcStatus>;
}

pub trait PostApi: Send + Sync {
    fn create(&self, author_id: u64, text: &str, request_id: &str) -> Result<Post, GrpcStatus>;
}
