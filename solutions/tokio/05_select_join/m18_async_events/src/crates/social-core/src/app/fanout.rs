use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot, watch};
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

enum Delivery {
    Notify(UserId, Notification),
    Flush(oneshot::Sender<()>),
}

pub struct Fanout {
    queue: mpsc::Sender<Delivery>,
    counters: Arc<Counters>,
}

impl Fanout {
    pub fn start(
        runtime: &Handle,
        notifiers: Notifiers,
        counters: Arc<Counters>,
        capacity: usize,
        stop: watch::Receiver<()>,
    ) -> Fanout {
        let (queue, deliveries) = mpsc::channel(capacity.max(1));
        runtime.spawn(deliver(deliveries, notifiers, stop));
        Fanout { queue, counters }
    }

    pub async fn published(
        &self,
        author: UserId,
        post: PostId,
        audience: Vec<UserId>,
    ) -> Result<(), AppError> {
        self.counters.posts.fetch_add(1, Ordering::Relaxed);
        let notification = Notification::NewPost { author, post };
        for user in audience {
            self.queue
                .send(Delivery::Notify(user, notification))
                .await
                .map_err(|_| AppError::Stopped(Op::Flush))?;
        }
        Ok(())
    }

    pub async fn flush(&self, limit: Duration) -> Result<(), AppError> {
        let (done, delivered) = oneshot::channel();
        let wait = async {
            self.queue
                .send(Delivery::Flush(done))
                .await
                .map_err(|_| AppError::Stopped(Op::Flush))?;
            delivered.await.map_err(|_| AppError::Stopped(Op::Flush))
        };
        timeout(limit, wait).await.map_err(|_| AppError::Timeout {
            op: Op::Flush,
            after: limit,
        })?
    }

    pub fn stats(&self, trend_refreshes: u64) -> Stats {
        Stats {
            posts: self.counters.posts.load(Ordering::Relaxed),
            notifications: self.counters.notifications.load(Ordering::Relaxed),
            trend_refreshes,
        }
    }
}

async fn deliver(
    mut deliveries: mpsc::Receiver<Delivery>,
    notifiers: Notifiers,
    mut stop: watch::Receiver<()>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => return,
            next = deliveries.recv() => match next {
                Some(Delivery::Notify(user, notification)) => {
                    let _ = catch_unwind(AssertUnwindSafe(|| notifiers.notify(user, notification)));
                }
                Some(Delivery::Flush(done)) => {
                    let _ = done.send(());
                }
                None => return,
            },
        }
    }
}
