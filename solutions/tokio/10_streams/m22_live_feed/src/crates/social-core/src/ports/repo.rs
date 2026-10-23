use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::chat::Message;
use crate::comments::Comment;
use crate::domain::{
    CommentId, Cursor, DomainError, PREVIEW_LEN, Post, PostId, PostKind, Timestamp, User, UserId,
    Visibility, preview,
};
use crate::trends::Trends;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostItem {
    pub id: PostId,
    pub author: UserId,
    pub author_name: Arc<str>,
    pub text: Arc<str>,
    pub kind: PostKind,
    pub visibility: Visibility,
    pub time: Timestamp,
    pub likes: usize,
}

impl PostItem {
    pub fn of(post: &Post, author: Option<&User>) -> PostItem {
        PostItem {
            id: post.id(),
            author: post.author(),
            author_name: author.map_or_else(|| Arc::from(""), User::shared_name),
            text: post.shared_text(),
            kind: post.kind(),
            visibility: post.visibility(),
            time: post.time(),
            likes: post.likes(),
        }
    }

    pub fn cursor(&self) -> Cursor {
        Cursor {
            time: self.time,
            post: self.id,
        }
    }
}

impl fmt::Display for PostItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PostKind::Text => write!(f, "[{}] {}", self.id.0, preview(&self.text, PREVIEW_LEN))?,
            PostKind::Repost(original) => write!(f, "[{}] репост [{}]", self.id.0, original.0)?,
        }
        write!(f, " (лайков: {})", self.likes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub post: PostId,
    pub at: Timestamp,
    pub audience: Vec<UserId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Liked {
    pub changed: bool,
    pub author: UserId,
    pub at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PostPage {
    pub posts: Vec<PostItem>,
    pub next: Option<Cursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeedSlice {
    pub posts: Vec<PostItem>,
    pub next: Option<PostId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FollowCounts {
    pub followers: usize,
    pub following: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AuthorStats {
    pub posts: usize,
    pub likes: usize,
}

pub trait UserRepo: Send + Sync {
    fn register(&self, name: String) -> BoxFuture<'_, Result<UserId, DomainError>>;
    fn user(&self, id: UserId) -> BoxFuture<'_, Option<User>>;
    fn find(&self, name: String) -> BoxFuture<'_, Option<User>>;
    fn follow(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>>;
    fn mute(&self, who: UserId, whom: UserId) -> BoxFuture<'_, Result<bool, DomainError>>;
    fn is_following(&self, who: UserId, whom: UserId) -> BoxFuture<'_, bool>;
    fn follow_counts(&self, id: UserId) -> BoxFuture<'_, FollowCounts>;
    fn count(&self) -> BoxFuture<'_, usize>;
}

pub trait PostRepo: Send + Sync {
    fn publish(
        &self,
        author: UserId,
        text: String,
        visibility: Visibility,
    ) -> BoxFuture<'_, Result<Published, DomainError>>;
    fn like(&self, user: UserId, post: PostId) -> BoxFuture<'_, Result<Liked, DomainError>>;
    fn delete(&self, who: UserId, post: PostId) -> BoxFuture<'_, Result<bool, DomainError>>;
    fn post(&self, id: PostId) -> BoxFuture<'_, Option<PostItem>>;
    fn feed(&self, viewer: UserId, skip: usize, take: usize) -> BoxFuture<'_, Vec<PostItem>>;
    fn feed_page(
        &self,
        viewer: UserId,
        before: Option<PostId>,
        size: usize,
    ) -> BoxFuture<'_, Result<FeedSlice, DomainError>>;
    fn feed_item(&self, viewer: UserId, post: PostId) -> BoxFuture<'_, Option<PostItem>>;
    fn by_author(
        &self,
        viewer: UserId,
        author: UserId,
        after: Option<Cursor>,
        size: usize,
    ) -> BoxFuture<'_, Result<PostPage, DomainError>>;
    fn author_stats(&self, author: UserId) -> BoxFuture<'_, AuthorStats>;
    fn comment(
        &self,
        author: UserId,
        post: PostId,
        text: String,
    ) -> BoxFuture<'_, Result<CommentId, DomainError>>;
    fn reply(
        &self,
        author: UserId,
        parent: CommentId,
        text: String,
    ) -> BoxFuture<'_, Result<CommentId, DomainError>>;
    fn replies(
        &self,
        viewer: UserId,
        post: PostId,
        parent: Option<CommentId>,
    ) -> BoxFuture<'_, Result<Vec<Comment>, DomainError>>;
    fn trend_snapshot(&self) -> BoxFuture<'_, (Trends, Timestamp)>;
    fn count(&self) -> BoxFuture<'_, usize>;
}

pub trait ChatRepo: Send + Sync {
    fn send(
        &self,
        from: UserId,
        to: UserId,
        text: String,
    ) -> BoxFuture<'_, Result<Message, DomainError>>;
    fn history(
        &self,
        user: UserId,
        peer: UserId,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<Message>, DomainError>>;
}

#[derive(Clone)]
pub struct Repos {
    pub users: Arc<dyn UserRepo>,
    pub posts: Arc<dyn PostRepo>,
    pub chats: Arc<dyn ChatRepo>,
}
