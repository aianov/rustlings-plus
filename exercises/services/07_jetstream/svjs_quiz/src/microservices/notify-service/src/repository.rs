use crate::common::{Incoming, Kind, Notification};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    next_id: u64,
    items: Vec<Notification>,
}

#[derive(Clone, Default)]
pub struct Store {
    inner: Arc<Mutex<State>>,
}

pub struct Tx<'a> {
    state: &'a mut State,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn transaction<R>(&self, work: impl FnOnce(&mut Tx<'_>) -> R) -> R {
        let mut guard = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut Tx { state: &mut guard })
    }
}

impl Tx<'_> {
    pub fn has_unread(&self, user: &str, kind: Kind, actor: &str, post_id: Option<u64>) -> bool {
        self.state.items.iter().any(|n| !n.read && n.user == user && n.kind == kind && n.actor == actor && n.post_id == post_id)
    }

    pub fn insert(&mut self, incoming: Incoming) -> Notification {
        self.state.next_id += 1;
        let notification = Notification {
            id: self.state.next_id,
            user: incoming.user,
            kind: incoming.kind,
            actor: incoming.actor,
            post_id: incoming.post_id,
            request_id: incoming.request_id,
            read: false,
        };
        self.state.items.push(notification.clone());
        notification
    }

    pub fn unread_count(&self, user: &str) -> u32 {
        self.state.items.iter().filter(|n| n.user == user && !n.read).count() as u32
    }

    pub fn mark_all_read(&mut self, user: &str) -> u32 {
        let mut changed = 0;
        for n in self.state.items.iter_mut().filter(|n| n.user == user && !n.read) {
            n.read = true;
            changed += 1;
        }
        changed
    }

    pub fn list(&self, user: &str) -> Vec<Notification> {
        self.state.items.iter().rev().filter(|n| n.user == user).cloned().collect()
    }
}
