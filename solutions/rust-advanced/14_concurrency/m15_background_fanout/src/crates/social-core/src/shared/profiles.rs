use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use crate::domain::{User, UserId};

#[derive(Default)]
pub struct Profiles {
    cache: RwLock<HashMap<UserId, Arc<User>>>,
}

impl Profiles {
    pub fn get(&self, id: UserId) -> Option<Arc<User>> {
        let cache = self.cache.read().unwrap_or_else(PoisonError::into_inner);
        cache.get(&id).cloned()
    }

    pub fn keep(&self, user: User) -> Arc<User> {
        let mut cache = self.cache.write().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(cache.entry(user.id()).or_insert_with(|| Arc::new(user)))
    }
}
