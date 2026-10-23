use std::collections::HashMap;

use crate::domain::{User, UserId};
use crate::ports::UserStore;

#[derive(Default)]
pub struct MemUserStore {
    users: HashMap<UserId, User>,
    names: HashMap<String, UserId>,
    last_id: u64,
}

impl MemUserStore {
    pub fn new() -> MemUserStore {
        MemUserStore::default()
    }
}

impl UserStore for MemUserStore {
    fn next_id(&mut self) -> UserId {
        self.last_id += 1;
        UserId(self.last_id)
    }

    fn insert(&mut self, user: User) {
        self.names.insert(user.name().to_string(), user.id());
        self.users.insert(user.id(), user);
    }

    fn get(&self, id: UserId) -> Option<&User> {
        self.users.get(&id)
    }

    fn id_by_name(&self, name: &str) -> Option<UserId> {
        self.names.get(name).copied()
    }

    fn count(&self) -> usize {
        self.users.len()
    }
}
