use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use contracts::{UserId, encode};
use social_core::{Notification, Notifier};
use tokio::sync::mpsc;

type Links = HashMap<UserId, Vec<(u64, mpsc::Sender<String>)>>;

#[derive(Default)]
pub struct Hub {
    links: Mutex<Links>,
    next: AtomicU64,
    dropped: AtomicU64,
}

pub struct Link {
    hub: Arc<Hub>,
    user: UserId,
    id: u64,
}

impl Hub {
    fn lock(&self) -> MutexGuard<'_, Links> {
        self.links.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn join(self: &Arc<Hub>, user: UserId, outbox: mpsc::Sender<String>) -> Link {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().entry(user).or_default().push((id, outbox));
        Link {
            hub: Arc::clone(self),
            user,
            id,
        }
    }

    pub fn connections(&self, user: UserId) -> usize {
        self.lock().get(&user).map_or(0, Vec::len)
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn leave(&self, user: UserId, id: u64) {
        let mut links = self.lock();
        if let Some(own) = links.get_mut(&user) {
            own.retain(|(link, _)| *link != id);
            if own.is_empty() {
                links.remove(&user);
            }
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.hub.leave(self.user, self.id);
    }
}

impl Notifier for Hub {
    fn notify(&self, user: UserId, notification: Notification) {
        let Ok(line) = encode(&notification.to_push(user)) else {
            return;
        };
        let links = self.lock();
        for (_, outbox) in links.get(&user).into_iter().flatten() {
            if outbox.try_send(line.clone()).is_err() {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
