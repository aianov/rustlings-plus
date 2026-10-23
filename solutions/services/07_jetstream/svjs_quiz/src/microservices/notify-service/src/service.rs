use crate::common::{Incoming, Notification};
use crate::repository::Store;

#[derive(Debug, PartialEq)]
pub enum Recorded {
    Stored(Notification),
    Duplicate,
}

pub struct NotifyService {
    store: Store,
}

impl NotifyService {
    pub fn new(store: Store) -> NotifyService {
        NotifyService { store }
    }

    pub fn record(&self, incoming: Incoming) -> Recorded {
        self.store.transaction(|tx| {
            if tx.has_unread(&incoming.user, incoming.kind, &incoming.actor, incoming.post_id) {
                Recorded::Duplicate
            } else {
                Recorded::Stored(tx.insert(incoming))
            }
        })
    }

    pub fn unread(&self, user: &str) -> u32 {
        self.store.transaction(|tx| tx.unread_count(user))
    }

    pub fn mark_read(&self, user: &str) -> u32 {
        self.store.transaction(|tx| tx.mark_all_read(user))
    }

    pub fn list(&self, user: &str) -> Vec<Notification> {
        self.store.transaction(|tx| tx.list(user))
    }
}
