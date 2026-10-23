use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::time::timeout;

use super::{AppError, Op};
use crate::domain::{PostId, UserId};
use crate::notify::{Notification, Notifier, Notifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub posts: u64,
    pub notifications: u64,
    pub trend_refreshes: u64,
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

#[derive(Default)]
struct Pending {
    count: AtomicUsize,
    idle: Notify,
}

struct Delivery {
    pending: Arc<Pending>,
}

impl Delivery {
    fn start(pending: &Arc<Pending>) -> Delivery {
        pending.count.fetch_add(1, Ordering::AcqRel);
        Delivery {
            pending: Arc::clone(pending),
        }
    }
}

impl Drop for Delivery {
    fn drop(&mut self) {
        if self.pending.count.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.pending.idle.notify_waiters();
        }
    }
}

pub struct Fanout {
    notifiers: Arc<Notifiers>,
    counters: Arc<Counters>,
    pending: Arc<Pending>,
}

impl Fanout {
    pub fn new(notifiers: Notifiers, counters: Arc<Counters>) -> Fanout {
        Fanout {
            notifiers: Arc::new(notifiers),
            counters,
            pending: Arc::default(),
        }
    }

    pub fn published(&self, runtime: &Handle, author: UserId, post: PostId, audience: Vec<UserId>) {
        self.counters.posts.fetch_add(1, Ordering::Relaxed);
        let notification = Notification::NewPost { author, post };
        for user in audience {
            let delivery = Delivery::start(&self.pending);
            let notifiers = Arc::clone(&self.notifiers);
            runtime.spawn(async move {
                let _delivery = delivery;
                notifiers.notify(user, notification);
            });
        }
    }

    pub async fn flush(&self, limit: Duration) -> Result<(), AppError> {
        timeout(limit, self.idle())
            .await
            .map_err(|_| AppError::Timeout {
                op: Op::Flush,
                after: limit,
            })
    }

    async fn idle(&self) {
        loop {
            let mut notified = pin!(self.pending.idle.notified());
            notified.as_mut().enable();
            if self.pending.count.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    pub fn stats(&self, trend_refreshes: u64) -> Stats {
        Stats {
            posts: self.counters.posts.load(Ordering::Relaxed),
            notifications: self.counters.notifications.load(Ordering::Relaxed),
            trend_refreshes,
        }
    }
}
