use std::time::{SystemTime, UNIX_EPOCH};

use contracts::{CommentId, PostId, UserId};

pub const MAX_POST_LEN: usize = 280;

pub const MAX_COMMENT_LEN: usize = 280;

pub const TRENDS_WINDOW: u64 = 24 * 60 * 60 * 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: PostId,
    pub author: UserId,
    pub author_name: String,
    pub text: String,
    pub likes: usize,
    pub at: u64,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub post: PostId,
    pub author: UserId,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Liked {
    pub changed: bool,
    pub author: UserId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trend {
    pub post: PostId,
    pub likes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub id: UserId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Graph {
    pub following: Vec<UserId>,
    pub muted: Vec<UserId>,
}

pub fn now_micros() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since.as_micros()).unwrap_or(u64::MAX)
}
