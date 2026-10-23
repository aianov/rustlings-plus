use std::future::Future;
use std::time::Duration;

use tokio::time::{Instant, sleep, timeout_at};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub interval: Duration,
    pub total: Duration,
}

impl Default for Policy {
    fn default() -> Policy {
        Policy {
            interval: Duration::from_millis(200),
            total: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GaveUp<E> {
    pub attempts: u32,
    pub last: Option<E>,
}

pub async fn wait_for<F, Fut, E>(policy: &Policy, mut probe: F) -> Result<u32, GaveUp<E>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    let deadline = Instant::now() + policy.total;
    let mut attempts = 0;
    loop {
        attempts += 1;
        let limit = deadline.max(Instant::now() + policy.interval);
        match timeout_at(limit, probe()).await {
            Ok(Ok(())) => return Ok(attempts),
            Ok(Err(last)) => {
                if Instant::now() + policy.interval > deadline {
                    return Err(GaveUp {
                        attempts,
                        last: Some(last),
                    });
                }
                sleep(policy.interval).await;
            }
            Err(..) => {
                return Err(GaveUp {
                    attempts,
                    last: None,
                });
            }
        }
    }
}
