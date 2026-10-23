use std::time::Duration;

use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    at: Instant,
}

impl Deadline {
    pub fn at(at: Instant) -> Deadline {
        Deadline { at }
    }

    pub fn within(budget: Duration) -> Deadline {
        Deadline {
            at: Instant::now() + budget,
        }
    }

    pub fn instant(&self) -> Instant {
        self.at
    }

    pub fn left(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    pub fn expired(&self) -> bool {
        self.left().is_zero()
    }

    pub fn reserve(&self, own: Duration) -> Deadline {
        Deadline {
            at: self.at.checked_sub(own).unwrap_or_else(Instant::now),
        }
    }

    pub fn cap(&self, budget: Duration) -> Deadline {
        Deadline {
            at: self.at.min(Instant::now() + budget),
        }
    }
}
