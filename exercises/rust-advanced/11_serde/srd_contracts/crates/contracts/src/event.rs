use crate::ids::{PostId, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone)]
pub enum Event {
    PostCreated { post: PostId, author: UserId },
    Followed { follower: UserId, followee: UserId },
    ChatMessage { chat: u64, from: UserId, text: String },
}
