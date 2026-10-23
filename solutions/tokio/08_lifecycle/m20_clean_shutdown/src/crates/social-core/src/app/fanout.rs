use std::hash::{BuildHasher, RandomState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use service_runtime::{CancellationToken, Restart, Stage, supervise};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{Instant, sleep_until, timeout};

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
    starts: AtomicU64,
}

impl Notifier for Counters {
    fn notify(&self, _user: UserId, _notification: Notification) {
        self.notifications.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Batching {
    pub size: usize,
    pub window: Duration,
}

enum Delivery {
    Notify(UserId, Notification),
    Flush(oneshot::Sender<()>),
}

type Batch = Vec<(UserId, Notification)>;

struct Worker {
    deliveries: Mutex<mpsc::Receiver<Delivery>>,
    notifiers: Notifiers,
    batching: Batching,
    stage: Stage,
}

pub struct Fanout {
    workers: Vec<mpsc::Sender<Delivery>>,
    hasher: RandomState,
    counters: Arc<Counters>,
}

impl Fanout {
    pub fn start(
        stage: &Stage,
        hard_stop: &CancellationToken,
        notifiers: Notifiers,
        counters: Arc<Counters>,
        sizes: (usize, usize),
        batching: Batching,
        restart: Restart,
    ) -> Fanout {
        let (workers, capacity) = sizes;
        let mut senders = Vec::with_capacity(workers.max(1));
        for _ in 0..workers.max(1) {
            let (sender, deliveries) = mpsc::channel(capacity.max(1));
            senders.push(sender);
            let worker = Arc::new(Worker {
                deliveries: Mutex::new(deliveries),
                notifiers: notifiers.clone(),
                batching: Batching {
                    size: batching.size.max(1),
                    window: batching.window,
                },
                stage: stage.clone(),
            });
            let started = Arc::clone(&counters);
            let token = hard_stop.clone();
            stage.tasks.spawn(async move {
                let run = move || {
                    let worker = Arc::clone(&worker);
                    started.starts.fetch_add(1, Ordering::Relaxed);
                    async move { worker.run().await }
                };
                supervise(run, token, restart).await
            });
        }
        Fanout {
            workers: senders,
            hasher: RandomState::new(),
            counters,
        }
    }

    fn worker_of(&self, user: UserId) -> &mpsc::Sender<Delivery> {
        let spread = self.hasher.hash_one(user) % self.workers.len() as u64;
        &self.workers[spread as usize]
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
            self.worker_of(user)
                .send(Delivery::Notify(user, notification))
                .await
                .map_err(|_| AppError::Stopped(Op::Flush))?;
        }
        Ok(())
    }

    pub async fn flush(&self, limit: Duration) -> Result<(), AppError> {
        let wait = async {
            let mut answers = Vec::with_capacity(self.workers.len());
            for worker in &self.workers {
                let (done, delivered) = oneshot::channel();
                worker
                    .send(Delivery::Flush(done))
                    .await
                    .map_err(|_| AppError::Stopped(Op::Flush))?;
                answers.push(delivered);
            }
            for delivered in answers {
                delivered.await.map_err(|_| AppError::Stopped(Op::Flush))?;
            }
            Ok(())
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

    pub fn restarts(&self) -> u64 {
        let starts = self.counters.starts.load(Ordering::Relaxed);
        starts.saturating_sub(self.workers.len() as u64)
    }
}

impl Worker {
    async fn run(&self) {
        let mut deliveries = self.deliveries.lock().await;
        let mut batch: Batch = Vec::with_capacity(self.batching.size);
        let mut due = Instant::now();
        loop {
            tokio::select! {
                biased;
                () = self.stage.token.cancelled() => {
                    deliveries.close();
                    while let Some(delivery) = deliveries.recv().await {
                        self.take(delivery, &mut batch);
                    }
                    self.deliver(&mut batch);
                    return;
                }
                next = deliveries.recv() => match next {
                    Some(delivery) => {
                        if batch.is_empty() {
                            due = Instant::now() + self.batching.window;
                        }
                        self.take(delivery, &mut batch);
                    }
                    None => {
                        self.deliver(&mut batch);
                        return;
                    }
                },
                () = sleep_until(due), if !batch.is_empty() => self.deliver(&mut batch),
            }
        }
    }

    fn take(&self, delivery: Delivery, batch: &mut Batch) {
        match delivery {
            Delivery::Notify(user, notification) => {
                batch.push((user, notification));
                if batch.len() >= self.batching.size {
                    self.deliver(batch);
                }
            }
            Delivery::Flush(done) => {
                self.deliver(batch);
                let _ = done.send(());
            }
        }
    }

    fn deliver(&self, batch: &mut Batch) {
        if !batch.is_empty() {
            self.notifiers.notify_batch(batch);
            batch.clear();
        }
    }
}
