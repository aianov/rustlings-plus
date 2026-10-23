mod entry;
mod memory;

use std::future::Future;
use std::pin::Pin;

use contracts::{CommentId, PostId, UserId};
use journal::JournalError;

use crate::common::{CommentText, Draft, Liked, Post};

pub use entry::{LinkEntry, PostEntry};
pub use memory::MemoryPosts;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait PostRepository: Send + Sync {
    fn insert(
        &self,
        author: UserId,
        author_name: String,
        draft: Draft,
        at: u64,
    ) -> BoxFuture<'_, Result<Post, JournalError>>;
    fn get(&self, id: PostId) -> BoxFuture<'_, Option<Post>>;
    fn like_if_absent(
        &self,
        user: UserId,
        post: PostId,
        at: u64,
    ) -> BoxFuture<'_, Result<Option<Liked>, JournalError>>;
    fn delete(&self, post: PostId) -> BoxFuture<'_, Result<bool, JournalError>>;
    fn by_authors(
        &self,
        authors: Vec<UserId>,
        skip: usize,
        take: usize,
    ) -> BoxFuture<'_, Vec<Post>>;
    fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: CommentText,
    ) -> BoxFuture<'_, Result<Option<CommentId>, JournalError>>;
    fn likes_since(&self, since: u64) -> BoxFuture<'_, Vec<(PostId, usize)>>;
}
