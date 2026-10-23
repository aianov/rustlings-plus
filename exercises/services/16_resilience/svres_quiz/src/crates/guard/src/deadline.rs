use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    at: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeadlineExceeded;

impl Deadline {
    pub fn after(budget: Duration) -> Deadline {
        Deadline { at: Instant::now() + budget }
    }

    pub fn remaining(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }
}

/// Выполняет `work` не дольше срока. Если срок вышел до начала, `work` не опрашивается.
pub async fn run_with<F: Future>(deadline: Deadline, work: F) -> Result<F::Output, DeadlineExceeded> {
    if deadline.expired() {
        return Err(DeadlineExceeded);
    }
    tokio::time::timeout_at(deadline.at, work).await.map_err(|_| DeadlineExceeded)
}
