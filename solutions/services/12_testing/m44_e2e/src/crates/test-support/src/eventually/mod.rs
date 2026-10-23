use std::fmt::{self, Debug};
use std::future::Future;
use std::time::Duration;

use tokio::time::{Instant, sleep};

pub const EVENTUALLY: Duration = Duration::from_secs(10);

const FIRST_PAUSE: Duration = Duration::from_millis(5);

const MAX_PAUSE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eventually {
    pub deadline: Duration,
    pub first: Duration,
    pub cap: Duration,
}

impl Default for Eventually {
    fn default() -> Eventually {
        Eventually {
            deadline: EVENTUALLY,
            first: FIRST_PAUSE,
            cap: MAX_PAUSE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GaveUp {
    pub what: String,
    pub deadline: Duration,
    pub attempts: u32,
    pub last: String,
}

impl fmt::Display for GaveUp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: не сбылось за {:?} и {} попыток, последний ответ: {}",
            self.what, self.deadline, self.attempts, self.last
        )
    }
}

impl std::error::Error for GaveUp {}

impl Eventually {
    pub fn within(deadline: Duration) -> Eventually {
        Eventually {
            deadline,
            ..Eventually::default()
        }
    }

    pub async fn until<T, E, F, Fut>(self, what: &str, mut check: F) -> Result<T, GaveUp>
    where
        E: Debug,
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let started = Instant::now();
        let mut pause = self.first;
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            let last = match check().await {
                Ok(value) => return Ok(value),
                Err(error) => error,
            };
            let spent = started.elapsed();
            if spent >= self.deadline {
                return Err(GaveUp {
                    what: what.to_string(),
                    deadline: self.deadline,
                    attempts,
                    last: format!("{last:?}"),
                });
            }
            sleep(pause.min(self.deadline - spent)).await;
            pause = (pause * 2).min(self.cap);
        }
    }
}

pub async fn eventually<T, E, F, Fut>(what: &str, check: F) -> Result<T, GaveUp>
where
    E: Debug,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    Eventually::default().until(what, check).await
}
