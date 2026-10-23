use serde::{Deserialize, Serialize};

use crate::ids::{PostId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "push", rename_all = "snake_case")]
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
    Message {
        to: UserId,
        from: UserId,
    },
}
