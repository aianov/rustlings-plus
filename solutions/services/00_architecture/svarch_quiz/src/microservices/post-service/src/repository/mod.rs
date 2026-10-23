mod memory;

use crate::common::Post;
use contracts::{PostId, UserId};
use futures::future::BoxFuture;

pub use memory::InMemoryPosts;

#[derive(Debug, PartialEq)]
pub enum RepoError {
    Conflict,
    Unavailable,
}

pub trait PostRepository: Send + Sync {
    fn create_if_absent(&self, author: UserId, text: String) -> BoxFuture<'_, Result<Post, RepoError>>;
    fn find(&self, id: PostId) -> BoxFuture<'_, Option<Post>>;
    fn by_author(&self, author: UserId) -> BoxFuture<'_, Vec<Post>>;
}
