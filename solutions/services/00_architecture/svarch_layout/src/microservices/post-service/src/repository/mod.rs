mod memory;

use crate::common::Post;
use contracts::{PostId, UserId};

pub use memory::InMemoryPosts;

pub trait PostRepository: Send + Sync {
    fn insert(&self, author: UserId, text: String) -> Post;
    fn find(&self, id: PostId) -> Option<Post>;
}
