use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen { probing: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejected {
    pub retry_after: Duration,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "предохранитель разомкнут, повтори через {:?}",
            self.retry_after
        )
    }
}

impl std::error::Error for Rejected {}

#[derive(Debug)]
struct Inner {
    state: BreakerState,
    threshold: u32,
    cool: Duration,
}

#[derive(Debug, Clone)]
pub struct Breaker {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
pub struct Pass {
    breaker: Breaker,
    probe: bool,
    done: bool,
}

impl Breaker {
    pub fn new(threshold: u32, cool: Duration) -> Breaker {
        Breaker {
            inner: Arc::new(Mutex::new(Inner {
                state: BreakerState::Closed { failures: 0 },
                threshold: threshold.max(1),
                cool,
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn state(&self) -> BreakerState {
        let mut inner = self.lock();
        if let BreakerState::Open { until } = inner.state {
            if Instant::now() >= until {
                inner.state = BreakerState::HalfOpen { probing: false };
            }
        }
        inner.state
    }

    pub fn allow(&self) -> Result<Pass, Rejected> {
        let state = self.state();
        let mut inner = self.lock();
        match state {
            BreakerState::Closed { .. } => Ok(Pass {
                breaker: self.clone(),
                probe: false,
                done: false,
            }),
            BreakerState::Open { until } => Err(Rejected {
                retry_after: until.saturating_duration_since(Instant::now()),
            }),
            BreakerState::HalfOpen { probing: true } => Err(Rejected {
                retry_after: inner.cool,
            }),
            BreakerState::HalfOpen { probing: false } => {
                inner.state = BreakerState::HalfOpen { probing: true };
                Ok(Pass {
                    breaker: self.clone(),
                    probe: true,
                    done: false,
                })
            }
        }
    }

    fn settle(&self, probe: bool, ok: bool) {
        let mut inner = self.lock();
        let cool = inner.cool;
        let threshold = inner.threshold;
        inner.state = match (inner.state, ok) {
            (_, true) => BreakerState::Closed { failures: 0 },
            (BreakerState::Closed { failures }, false) if failures + 1 < threshold => {
                BreakerState::Closed {
                    failures: failures + 1,
                }
            }
            (BreakerState::HalfOpen { .. }, false) if !probe => inner.state,
            (_, false) => BreakerState::Open {
                until: Instant::now() + cool,
            },
        };
    }
}

impl Pass {
    pub fn success(mut self) {
        self.done = true;
        self.breaker.settle(self.probe, true);
    }

    pub fn failure(mut self) {
        self.done = true;
        self.breaker.settle(self.probe, false);
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        if !self.done {
            self.breaker.settle(self.probe, false);
        }
    }
}
