use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{PoisonError, RwLock, Weak};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep_until, timeout};

use super::{App, AppError, Inner, Op, Settings};
use crate::events::{EventStream, Next, Topic};
use crate::trends::Trend;

pub const MAX_TRENDS: usize = 100;

pub(super) type Requests = mpsc::Sender<Refresh>;

pub(super) struct Refresh {
    reply: oneshot::Sender<Result<(), AppError>>,
}

#[derive(Default)]
pub struct Board {
    top: RwLock<Vec<Trend>>,
    refreshes: AtomicU64,
    missed: AtomicU64,
}

impl Board {
    fn top(&self, n: usize) -> Vec<Trend> {
        let top = self.top.read().unwrap_or_else(PoisonError::into_inner);
        top.iter().take(n).copied().collect()
    }

    fn replace(&self, fresh: Vec<Trend>) {
        *self.top.write().unwrap_or_else(PoisonError::into_inner) = fresh;
        self.refreshes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn refreshes(&self) -> u64 {
        self.refreshes.load(Ordering::Relaxed)
    }

    pub fn missed(&self) -> u64 {
        self.missed.load(Ordering::Relaxed)
    }
}

impl App {
    pub fn trends(&self, n: usize) -> Vec<Trend> {
        self.inner.board.top(n)
    }

    pub fn missed_events(&self) -> u64 {
        self.inner.board.missed()
    }

    pub async fn refresh_trends(&self) -> Result<(), AppError> {
        let after = self.inner.config.op_timeout;
        let (reply, answer) = oneshot::channel();
        let ask = async {
            self.inner
                .trends
                .send(Refresh { reply })
                .await
                .map_err(|_| AppError::Stopped(Op::Trends))?;
            answer.await.map_err(|_| AppError::Stopped(Op::Trends))?
        };
        timeout(after, ask).await.map_err(|_| AppError::Timeout {
            op: Op::Trends,
            after,
        })?
    }

    async fn recount_trends(&self) -> Result<(), AppError> {
        let window = self.settings().trends_window;
        let (likes, now) = self.inner.repos.posts.trend_snapshot().await;
        let fresh = self
            .run_heavy(Op::Trends, move || likes.top(now, window, MAX_TRENDS))
            .await?;
        self.inner.board.replace(fresh);
        Ok(())
    }
}

pub(super) struct Ticker {
    pub(super) app: Weak<Inner>,
    pub(super) requests: mpsc::Receiver<Refresh>,
    pub(super) stop: watch::Receiver<()>,
    pub(super) settings: watch::Receiver<Settings>,
    pub(super) events: EventStream,
    pub(super) every: Duration,
    pub(super) quiet: Duration,
}

async fn recount(app: &Weak<Inner>) -> Result<(), AppError> {
    let Some(inner) = app.upgrade() else {
        return Err(AppError::Stopped(Op::Trends));
    };
    App { inner }.recount_trends().await
}

impl Ticker {
    pub(super) async fn run(mut self) {
        let every = self.every.max(Duration::from_millis(1));
        let mut ticks = interval_at(Instant::now() + every, every);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut quiet = pin!(sleep_until(Instant::now() + every));
        let mut dirty = false;
        loop {
            tokio::select! {
                biased;
                _ = self.stop.changed() => return,
                request = self.requests.recv() => {
                    let Some(Refresh { reply }) = request else {
                        return;
                    };
                    let _ = reply.send(recount(&self.app).await);
                }
                changed = self.settings.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let _ = recount(&self.app).await;
                }
                _ = ticks.tick() => {
                    let _ = recount(&self.app).await;
                }
                _ = &mut quiet, if dirty => {
                    dirty = false;
                    let _ = recount(&self.app).await;
                }
                next = self.events.next() => match next {
                    Next::Event(event) if event.topic() == Topic::Liked => {
                        dirty = true;
                        quiet.as_mut().reset(Instant::now() + self.quiet);
                    }
                    Next::Event(_) => {}
                    Next::Missed(missed) => {
                        if let Some(inner) = self.app.upgrade() {
                            inner.board.missed.fetch_add(missed, Ordering::Relaxed);
                        }
                        dirty = true;
                        quiet.as_mut().reset(Instant::now() + self.quiet);
                    }
                    Next::Closed => return,
                },
            }
        }
    }
}
