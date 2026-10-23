use std::sync::Arc;

use contracts::{UserId, Username};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub name: Username,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Avatar {
    pub content_type: String,
    pub bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relations {
    pub following: Vec<UserId>,
    pub followers: Vec<UserId>,
    pub muted: Vec<UserId>,
    pub audience: Vec<UserId>,
}
