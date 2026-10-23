use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use worker_pool::ThreadPool;

use crate::domain::{PostId, UserId};
use crate::error::Error;
use crate::notify::{Notification, Notifier, Notifiers};

pub const FANOUT_WORKERS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub posts: u64,
    pub notifications: u64,
}

#[derive(Default)]
pub struct Counters {
    posts: AtomicU64,
    notifications: AtomicU64,
}

impl Notifier for Counters {
    fn notify(&self, _user: UserId, _notification: Notification) {
        self.notifications.fetch_add(1, Ordering::Relaxed);
    }
}

pub struct Fanout {
    pool: ThreadPool,
    notifiers: Arc<Notifiers>,
    counters: Arc<Counters>,
}

impl Fanout {
    pub fn start(notifiers: Notifiers, counters: Arc<Counters>) -> crate::Result<Fanout> {
        Ok(Fanout {
            pool: ThreadPool::new(FANOUT_WORKERS).map_err(Error::Start)?,
            notifiers: Arc::new(notifiers),
            counters,
        })
    }

    pub fn published(&self, author: UserId, post: PostId, audience: Vec<UserId>) {
        self.counters.posts.fetch_add(1, Ordering::Relaxed);
        let notification = Notification::NewPost { author, post };
        for user in audience {
            let notifiers = Arc::clone(&self.notifiers);
            let job = move || notifiers.notify(user, notification);
            if self.pool.execute(job).is_err() {
                return;
            }
        }
    }

    pub fn flush(&self) {
        self.pool.wait_idle();
    }

    pub fn stats(&self) -> Stats {
        Stats {
            posts: self.counters.posts.load(Ordering::Relaxed),
            notifications: self.counters.notifications.load(Ordering::Relaxed),
        }
    }
}
