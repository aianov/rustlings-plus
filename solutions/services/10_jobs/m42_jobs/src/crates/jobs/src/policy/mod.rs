use std::time::Duration;

use crate::job::{Failure, Outcome};

pub const MAX_ATTEMPTS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Done,
    Retry(Duration),
    Dead(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub first: Duration,
    pub cap: Duration,
    pub attempts: u64,
}

impl Default for Policy {
    fn default() -> Policy {
        Policy {
            first: Duration::from_secs(1),
            cap: Duration::from_secs(60),
            attempts: MAX_ATTEMPTS,
        }
    }
}

impl Policy {
    pub fn backoff(&self, attempt: u64) -> Duration {
        let doublings = u32::try_from(attempt.saturating_sub(1).min(32)).unwrap_or(32);
        let factor = 1u32.checked_shl(doublings).unwrap_or(u32::MAX);
        self.first.saturating_mul(factor).min(self.cap)
    }

    pub fn decide(&self, outcome: &Outcome, attempt: u64) -> Decision {
        match outcome {
            Ok(()) => Decision::Done,
            Err(Failure::Permanent(reason)) => Decision::Dead(reason.clone()),
            Err(Failure::Transient(reason)) if attempt >= self.attempts => Decision::Dead(format!(
                "исчерпано попыток: {attempt}, последняя ошибка: {reason}"
            )),
            Err(Failure::Transient(..)) => Decision::Retry(self.backoff(attempt)),
        }
    }
}
