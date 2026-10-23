mod decode;
mod views;

use serde::Serialize;

use crate::ids::{PostId, UserId};
use crate::username::Username;

pub use decode::decode_call;

pub use views::{
    Changed, ChatHistory, CommentLineView, CommentNodeView, CommentView, DailyView, DayView,
    FeedPage, LinkView, MessageView, PeopleView, PostView, ProfileView, RankView, Reply, StatsView,
    ThreadView, TopView, UserView,
};

pub const METHOD_NAMES: [&str; 16] = [
    "users.register",
    "users.find",
    "users.follow",
    "users.mute",
    "users.profile",
    "posts.create",
    "posts.like",
    "posts.edit",
    "posts.delete",
    "posts.feed",
    "posts.comment",
    "chat.send",
    "chat.history",
    "notify.unread",
    "notify.list",
    "notify.read",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Call {
    Register {
        username: Username,
    },
    FindUser {
        username: Username,
    },
    Follow {
        user: UserId,
        target: UserId,
    },
    Mute {
        user: UserId,
        target: UserId,
    },
    Profile {
        target: UserId,
    },
    CreatePost {
        user: UserId,
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        links: Vec<LinkView>,
    },
    Like {
        user: UserId,
        post: PostId,
    },
    EditPost {
        user: UserId,
        post: PostId,
        text: String,
    },
    DeletePost {
        user: UserId,
        post: PostId,
    },
    Feed {
        user: UserId,
        page: usize,
        size: usize,
    },
    Comment {
        user: UserId,
        post: PostId,
        text: String,
    },
    SendMessage {
        user: UserId,
        peer: UserId,
        text: String,
    },
    History {
        user: UserId,
        peer: UserId,
        limit: usize,
    },
    Unread {
        user: UserId,
    },
    Notices {
        user: UserId,
        limit: u32,
    },
    ReadNotices {
        user: UserId,
    },
}

impl Call {
    pub fn method(&self) -> &'static str {
        match self {
            Call::Register { .. } => "users.register",
            Call::FindUser { .. } => "users.find",
            Call::Follow { .. } => "users.follow",
            Call::Mute { .. } => "users.mute",
            Call::Profile { .. } => "users.profile",
            Call::CreatePost { .. } => "posts.create",
            Call::Like { .. } => "posts.like",
            Call::EditPost { .. } => "posts.edit",
            Call::DeletePost { .. } => "posts.delete",
            Call::Feed { .. } => "posts.feed",
            Call::Comment { .. } => "posts.comment",
            Call::SendMessage { .. } => "chat.send",
            Call::History { .. } => "chat.history",
            Call::Unread { .. } => "notify.unread",
            Call::Notices { .. } => "notify.list",
            Call::ReadNotices { .. } => "notify.read",
        }
    }
}
