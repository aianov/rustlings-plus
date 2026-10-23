use serde::{Deserialize, Serialize};

use crate::ids::{CommentId, PostId, UserId};
use crate::notice::{MarkedView, NoticesView, UnreadView};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserView {
    pub id: UserId,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

impl UserView {
    pub fn named(id: UserId, username: &str) -> UserView {
        UserView {
            id,
            username: username.to_string(),
            display_name: None,
            bio: None,
            avatar: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkView {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostView {
    pub id: PostId,
    pub author: UserId,
    pub author_name: String,
    pub text: String,
    pub likes: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentView {
    pub id: CommentId,
    pub post: PostId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub from: UserId,
    pub to: UserId,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changed {
    pub changed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedPage {
    pub posts: Vec<PostView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<UserView>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatHistory {
    pub messages: Vec<MessageView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsView {
    pub followers: usize,
    pub following: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileView {
    pub user: UserView,
    pub posts: Option<Vec<PostView>>,
    pub stats: Option<StatsView>,
    pub partial: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Reply {
    User(UserView),
    Post(PostView),
    Changed(Changed),
    Feed(FeedPage),
    Message(MessageView),
    History(ChatHistory),
    Profile(ProfileView),
    Comment(CommentView),
    Unread(UnreadView),
    Notices(NoticesView),
    Marked(MarkedView),
}
