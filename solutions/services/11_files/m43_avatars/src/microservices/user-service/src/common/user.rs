use contracts::{UserId, Username};

use super::{Bio, DisplayName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub name: Username,
    pub display_name: Option<DisplayName>,
    pub bio: Option<Bio>,
}

impl User {
    pub fn new(id: UserId, name: Username) -> User {
        User {
            id,
            name,
            display_name: None,
            bio: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Avatar {
    pub key: String,
    pub content_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relations {
    pub following: Vec<UserId>,
    pub followers: Vec<UserId>,
    pub muted: Vec<UserId>,
    pub audience: Vec<UserId>,
}
