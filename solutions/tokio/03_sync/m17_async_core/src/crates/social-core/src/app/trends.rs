use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior, interval_at};

use super::{App, AppError, Op};
use crate::ports::{PostStore, UserStore};
use crate::trends::{TRENDS_WINDOW, Trend};

pub const MAX_TRENDS: usize = 100;

#[derive(Default)]
pub struct Board {
    top: RwLock<Vec<Trend>>,
    refreshes: AtomicU64,
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
}

impl<U, P> App<U, P>
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    pub fn trends(&self, n: usize) -> Vec<Trend> {
        self.inner.board.top(n)
    }

    pub async fn refresh_trends(&self) -> Result<(), AppError> {
        let (likes, now) = self.read(|network| (network.trend_likes(), network.now()));
        let fresh = self
            .run_heavy(Op::Trends, move || {
                likes.top(now, TRENDS_WINDOW, MAX_TRENDS)
            })
            .await?;
        self.inner.board.replace(fresh);
        Ok(())
    }
}

pub fn spawn_ticker<U, P>(app: &App<U, P>)
where
    U: UserStore + Send + 'static,
    P: PostStore + Send + 'static,
{
    let every = app.inner.config.trends_every.max(Duration::from_millis(1));
    let weak = Arc::downgrade(&app.inner);
    app.inner.runtime.spawn(async move {
        let mut ticks = interval_at(Instant::now() + every, every);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let _ = App { inner }.refresh_trends().await;
        }
    });
}
