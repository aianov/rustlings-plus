use contracts::{
    DEFAULT_FEED_LIMIT, DEFAULT_HISTORY_LIMIT, DEFAULT_RECENT, LinkView, PostId, UserId, Username,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FindUser {
    pub username: Username,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub target: UserId,
}

#[derive(Debug, Deserialize)]
pub struct NewPost {
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub links: Vec<LinkView>,
}

#[derive(Debug, Deserialize)]
pub struct PostRef {
    pub post: PostId,
}

#[derive(Debug, Deserialize)]
pub struct EditPost {
    pub post: PostId,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct FeedAsk {
    #[serde(default = "first_page")]
    pub page: usize,
    #[serde(default = "feed_size")]
    pub size: usize,
}

#[derive(Debug, Deserialize)]
pub struct CommentAsk {
    pub post: PostId,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct NoticesAsk {
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Letter {
    pub peer: UserId,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct PeerAsk {
    pub peer: UserId,
}

#[derive(Debug, Deserialize)]
pub struct HistoryAsk {
    pub peer: UserId,
    #[serde(default = "history_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct WatchAsk {
    #[serde(default = "recent")]
    pub recent: usize,
}

#[derive(Debug, Deserialize)]
pub struct Nothing {}

fn first_page() -> usize {
    1
}

fn feed_size() -> usize {
    DEFAULT_FEED_LIMIT
}

fn history_limit() -> usize {
    DEFAULT_HISTORY_LIMIT
}

fn recent() -> usize {
    DEFAULT_RECENT
}
