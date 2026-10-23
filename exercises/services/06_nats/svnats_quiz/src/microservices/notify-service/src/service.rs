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
        todo!()
    }

    pub fn unread(&self, user: &str) -> u32 {
        todo!()
    }

    pub fn mark_read(&self, user: &str) -> u32 {
        todo!()
    }

    pub fn list(&self, user: &str) -> Vec<Notification> {
        todo!()
    }
}
