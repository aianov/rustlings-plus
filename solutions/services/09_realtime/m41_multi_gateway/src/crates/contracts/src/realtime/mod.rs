use serde::{Deserialize, Serialize};

use crate::ids::UserId;

pub const MAX_REVOKED: usize = 1000;

pub const REALTIME_METHODS: [&str; 2] = ["chat.typing", "users.online"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypingNote {
    pub from: UserId,
    pub to: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRevoked {
    pub user: UserId,
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineView {
    pub user: UserId,
    pub online: bool,
}
