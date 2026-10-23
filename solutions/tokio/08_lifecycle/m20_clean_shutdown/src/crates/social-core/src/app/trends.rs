use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use service_runtime::Stage;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep_until, timeout};

use super::{App, AppError, Inner, Op, Settings};
use crate::events::{EventStream, Next, Topic};
use crate::trends::Trend;

pub const MAX_TRENDS: usize = 100;

enum Request {
    Refresh(oneshot::Sender<Result<(), AppError>>),
    Top(usize, oneshot::Sender<Vec<Trend>>),
}

#[derive(Default)]
pub(super) struct TrendCounters {
    refreshes: AtomicU64,
    missed: AtomicU64,
}

impl TrendCounters {
    pub(super) fn refreshes(&self) -> u64 {
        self.refreshes.load(Ordering::Relaxed)
    }

    pub(super) fn missed(&self) -> u64 {
        self.missed.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct TrendsHandle {
    requests: mpsc::Sender<Request>,
    board: watch::Receiver<Arc<[Trend]>>,
}

impl TrendsHandle {
    pub fn latest(&self, n: usize) -> Vec<Trend> {
        self.board.borrow().iter().take(n).copied().collect()
    }

    pub async fn top(&self, n: usize) -> Result<Vec<Trend>, AppError> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Top(n, reply))
            .await
            .map_err(|_| AppError::Stopped(Op::Trends))?;
        answer.await.map_err(|_| AppError::Stopped(Op::Trends))
    }

    pub async fn refresh(&self) -> Result<(), AppError> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Refresh(reply))
            .await
            .map_err(|_| AppError::Stopped(Op::Trends))?;
        answer.await.map_err(|_| AppError::Stopped(Op::Trends))?
    }
}

pub(super) struct TrendsActor {
    requests: mpsc::Receiver<Request>,
    board: watch::Sender<Arc<[Trend]>>,
}

pub(super) fn trends_actor() -> (TrendsHandle, TrendsActor) {
    let (requests, inbox) = mpsc::channel(8);
    let (board, latest) = watch::channel(Arc::from(Vec::new()));
    (
        TrendsHandle {
            requests,
            board: latest,
        },
        TrendsActor {
            requests: inbox,
            board,
        },
    )
}

async fn recount(app: &Weak<Inner>) -> Result<Vec<Trend>, AppError> {
    let Some(inner) = app.upgrade() else {
        return Err(AppError::Stopped(Op::Trends));
    };
    App { inner }.recount_trends().await
}

impl TrendsActor {
    pub(super) async fn run(
        mut self,
        app: Weak<Inner>,
        counters: Arc<TrendCounters>,
        stage: Stage,
    ) {
        loop {
            tokio::select! {
                biased;
                () = stage.token.cancelled() => return,
                request = self.requests.recv() => match request {
                    Some(Request::Refresh(reply)) => {
                        let result = recount(&app).await.map(|fresh| {
                            self.board.send_replace(Arc::from(fresh));
                            counters.refreshes.fetch_add(1, Ordering::Relaxed);
                        });
                        let _ = reply.send(result);
                    }
                    Some(Request::Top(n, reply)) => {
                        let top = self.board.borrow().iter().take(n).copied().collect();
                        let _ = reply.send(top);
                    }
                    None => return,
                },
            }
        }
    }
}

pub(super) struct Ticker {
    pub(super) trends: TrendsHandle,
    pub(super) counters: Arc<TrendCounters>,
    pub(super) settings: watch::Receiver<Settings>,
    pub(super) events: EventStream,
    pub(super) every: Duration,
    pub(super) quiet: Duration,
}

impl Ticker {
    pub(super) async fn run(mut self, stage: Stage) {
        let every = self.every.max(Duration::from_millis(1));
        let mut ticks = interval_at(Instant::now() + every, every);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut quiet = pin!(sleep_until(Instant::now() + every));
        let mut dirty = false;
        loop {
            tokio::select! {
                biased;
                () = stage.token.cancelled() => return,
                changed = self.settings.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let _ = self.trends.refresh().await;
                }
                _ = ticks.tick() => {
                    let _ = self.trends.refresh().await;
                }
                () = &mut quiet, if dirty => {
                    dirty = false;
                    let _ = self.trends.refresh().await;
                }
                next = self.events.next() => match next {
                    Next::Event(event) if event.topic() == Topic::Liked => {
                        dirty = true;
                        quiet.as_mut().reset(Instant::now() + self.quiet);
                    }
                    Next::Event(_) => {}
                    Next::Missed(missed) => {
                        self.counters.missed.fetch_add(missed, Ordering::Relaxed);
                        dirty = true;
                        quiet.as_mut().reset(Instant::now() + self.quiet);
                    }
                    Next::Closed => return,
                },
            }
        }
    }
}

impl App {
    pub fn trends(&self, n: usize) -> Vec<Trend> {
        self.inner.trends.latest(n)
    }

    pub fn trends_handle(&self) -> TrendsHandle {
        self.inner.trends.clone()
    }

    pub fn missed_events(&self) -> u64 {
        self.inner.trend_counters.missed()
    }

    pub async fn refresh_trends(&self) -> Result<(), AppError> {
        let after = self.inner.config.op_timeout;
        timeout(after, self.inner.trends.refresh())
            .await
            .map_err(|_| AppError::Timeout {
                op: Op::Trends,
                after,
            })?
    }

    async fn recount_trends(&self) -> Result<Vec<Trend>, AppError> {
        let window = self.settings().trends_window;
        let (likes, now) = self.inner.repos.posts.trend_snapshot().await;
        self.run_heavy(Op::Trends, move || likes.top(now, window, MAX_TRENDS))
            .await
    }
}
