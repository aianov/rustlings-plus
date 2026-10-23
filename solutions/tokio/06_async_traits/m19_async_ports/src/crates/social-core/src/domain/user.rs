use std::fmt;
use std::sync::Arc;

use contracts::Username;

use super::{DomainError, UserId};

#[derive(Debug, Clone)]
pub struct User {
    id: UserId,
    name: Arc<str>,
    bio: String,
}

impl User {
    pub fn new(id: UserId, name: String) -> User {
        User {
            id,
            name: name.into(),
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
        self.name = name.into();
    }

    pub fn shared_name(&self) -> Arc<str> {
        Arc::clone(&self.name)
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

pub fn validate_name(name: &str) -> Result<String, DomainError> {
    Ok(Username::try_from(name)?.into())
}
