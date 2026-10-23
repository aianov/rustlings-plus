use serde::{Deserialize, Serialize};

use crate::ids::{PostId, UserId};
use crate::push::Push;

pub const DEFAULT_NOTICES: u32 = 20;

pub const MAX_NOTICES: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    Followed,
    Liked,
    Commented,
    NewPost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeView {
    pub id: u64,
    pub to: UserId,
    pub kind: NoticeKind,
    pub actor: UserId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<PostId>,
    pub read: bool,
}

impl NoticeView {
    pub fn push(&self) -> Option<Push> {
        let (to, by) = (self.to, self.actor);
        match (self.kind, self.post) {
            (NoticeKind::Followed, _) => Some(Push::Followed { to, by }),
            (NoticeKind::Liked, Some(post)) => Some(Push::Liked { to, by, post }),
            (NoticeKind::Commented, Some(post)) => Some(Push::Commented { to, by, post }),
            (NoticeKind::NewPost, Some(post)) => Some(Push::NewPost {
                to,
                author: by,
                post,
            }),
            (_, None) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnreadView {
    pub unread: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticesView {
    pub notices: Vec<NoticeView>,
    pub unread: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkedView {
    pub marked: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeAsk {
    pub user: UserId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}
