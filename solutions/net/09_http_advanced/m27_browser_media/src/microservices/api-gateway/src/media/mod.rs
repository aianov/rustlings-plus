use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use contracts::UserId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Avatar {
    pub content_type: String,
    pub bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
pub struct Avatars {
    stored: Mutex<HashMap<UserId, Avatar>>,
}

impl Avatars {
    fn lock(&self) -> MutexGuard<'_, HashMap<UserId, Avatar>> {
        self.stored.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn put(&self, owner: UserId, avatar: Avatar) -> bool {
        self.lock().insert(owner, avatar).is_none()
    }

    pub fn get(&self, owner: UserId) -> Option<Avatar> {
        self.lock().get(&owner).cloned()
    }

    pub fn remove(&self, owner: UserId) -> bool {
        self.lock().remove(&owner).is_some()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
