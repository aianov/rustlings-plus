mod bulk;
mod drafts;
mod entry;
mod fulltext;
mod memory;
mod outbox;
mod pg;
mod pg_drafts;
mod pg_repo;
mod pins;
mod queries;
mod schema;
mod search;
mod state;
mod threads;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Cause, CommentId, OutboxRow, PostId, UserId};
use journal::JournalError;
use tokio::sync::Notify;

use crate::common::{CommentText, Draft, Liked, Post, PostText};

pub use bulk::{tags_with, write_links, write_tags};
pub use drafts::DraftRepository;
pub use entry::{LinkEntry, PostEntry, PostLine};
pub use fulltext::{FullText, Hit};
pub use memory::{DEFAULT_NODE, LIKE_BATCH, MemoryPosts};
pub use pg::PgPosts;
pub use pins::{MAX_PINS, PinQueries, Pinned};
pub use queries::{BY_IDS, DAY_MICROS, day_name};
pub use schema::MIGRATIONS;
pub use search::{AUTHOR_PAGE, EXPORT_BATCH, Search, SearchQueries, Sort};
pub use threads::{Line, Node, Rank, ThreadQueries, WEEK_MICROS};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait PostRepository: Send + Sync {
    fn insert(
        &self,
        author: UserId,
        author_name: String,
        draft: Draft,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Post, JournalError>>;
    fn get(&self, id: PostId) -> BoxFuture<'_, Option<Post>>;
    fn like_if_absent(
        &self,
        user: UserId,
        post: PostId,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<Liked>, JournalError>>;
    fn edit(
        &self,
        post: PostId,
        text: PostText,
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<Post>, JournalError>>;
    fn delete(&self, post: PostId, cause: &Cause) -> BoxFuture<'_, Result<bool, JournalError>>;
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
        cause: &Cause,
    ) -> BoxFuture<'_, Result<Option<CommentId>, JournalError>>;
    fn likes_since(&self, since: u64) -> BoxFuture<'_, Vec<(PostId, usize)>>;
    fn flush(&self) -> BoxFuture<'_, Result<usize, JournalError>>;
}

pub trait PostQueries: Send + Sync {
    fn by_ids(
        &self,
        viewer: Option<UserId>,
        ids: Vec<PostId>,
        hidden: Vec<UserId>,
    ) -> BoxFuture<'_, Vec<(Post, bool)>>;
    fn author_totals(&self, author: UserId) -> BoxFuture<'_, (u64, u64)>;
    fn liked(&self, viewer: UserId, posts: Vec<PostId>) -> BoxFuture<'_, Vec<PostId>>;
    fn daily(&self, days: u32, now: u64) -> BoxFuture<'_, Vec<(String, u64, u64)>>;
}

pub trait OutboxRepository: Send + Sync {
    fn unpublished(&self, limit: usize) -> BoxFuture<'_, Vec<OutboxRow>>;
    fn mark_published(&self, through: u64) -> BoxFuture<'_, Result<(), JournalError>>;
    fn waiting(&self) -> BoxFuture<'_, usize>;
    fn signal(&self) -> Arc<Notify>;
}
