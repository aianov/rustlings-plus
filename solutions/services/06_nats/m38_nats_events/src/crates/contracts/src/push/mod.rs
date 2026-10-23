use serde::{Deserialize, Serialize};

use crate::api::PostView;
use crate::ids::{PostId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum Push {
    Followed {
        to: UserId,
        by: UserId,
    },
    Liked {
        to: UserId,
        by: UserId,
        post: PostId,
    },
    Commented {
        to: UserId,
        by: UserId,
        post: PostId,
    },
    Message {
        to: UserId,
        from: UserId,
    },
    NewPost {
        to: UserId,
        author: UserId,
        post: PostId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum FeedPush {
    FeedPost {
        to: UserId,
        post: PostView,
        live: bool,
    },
    FeedCaughtUp {
        to: UserId,
    },
}
