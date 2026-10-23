use std::borrow::Cow;
use std::fmt;

use super::DomainError;

pub const MAX_NAME_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub u64);

#[derive(Debug)]
pub struct User {
    id: UserId,
    name: String,
    bio: String,
}

impl User {
    pub fn new(id: UserId, name: String) -> User {
        User {
            id,
            name,
            bio: String::new(),
        }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bio(&self) -> &str {
        &self.bio
    }

    pub fn rename(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_bio(&mut self, bio: String) {
        self.bio = bio;
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.bio.is_empty() {
            write!(f, "@{}", self.name)
        } else {
            write!(f, "@{}: {}", self.name, self.bio)
        }
    }
}

pub fn normalize_name(name: &str) -> Cow<'_, str> {
    let trimmed = name.trim();
    if trimmed.chars().any(char::is_uppercase) {
        Cow::Owned(trimmed.to_lowercase())
    } else {
        Cow::Borrowed(trimmed)
    }
}

pub fn validate_name(name: &str) -> Result<String, DomainError> {
    let name = normalize_name(name);
    if name.is_empty() {
        return Err(DomainError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(DomainError::NameTooLong);
    }
    Ok(name.into_owned())
}
