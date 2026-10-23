use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct UserId(pub u64);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PostId(pub u64);
