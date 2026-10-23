use contracts::{
    CommentId, DEFAULT_FEED_LIMIT, DEFAULT_HISTORY_LIMIT, DEFAULT_RECENT, LinkView, PostId, UserId,
    Username,
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
    #[serde(default)]
    pub idempotency_key: Option<String>,
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
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommentAsk {
    pub post: PostId,
    pub text: String,
    #[serde(default)]
    pub parent: Option<CommentId>,
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
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WatchAsk {
    #[serde(default = "recent")]
    pub recent: usize,
}

#[derive(Debug, Deserialize)]
pub struct Nothing {}

#[derive(Debug, Deserialize)]
pub struct RelatedAsk {
    pub target: UserId,
    #[serde(default)]
    pub after: Option<UserId>,
    #[serde(default = "people")]
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
pub struct DailyAsk {
    #[serde(default = "week")]
    pub days: u32,
}

#[derive(Debug, Deserialize)]
pub struct CommentRef {
    pub comment: CommentId,
}

#[derive(Debug, Deserialize)]
pub struct ThreadAsk {
    pub post: PostId,
    #[serde(default)]
    pub depth: u32,
}

#[derive(Debug, Deserialize)]
pub struct SearchAsk {
    #[serde(default)]
    pub author: Option<UserId>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
pub struct AuthorAsk {
    pub target: UserId,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "author_page")]
    pub limit: u32,
}

fn author_page() -> u32 {
    10
}

#[derive(Debug, Deserialize)]
pub struct QueryAsk {
    pub query: String,
    #[serde(default)]
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
pub struct TopAsk {
    #[serde(default)]
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
pub struct SuggestAsk {
    #[serde(default = "people")]
    pub limit: u32,
}

fn people() -> u32 {
    20
}

fn week() -> u32 {
    7
}

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
