use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::time::sleep;

use super::deadline::Deadline;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub base: Duration,
    pub cap: Duration,
}

impl Default for RetryPolicy {
    fn default() -> RetryPolicy {
        RetryPolicy {
            attempts: 3,
            base: Duration::from_millis(20),
            cap: Duration::from_millis(500),
        }
    }
}

pub fn full_jitter(policy: &RetryPolicy, retry: u32, random: u64) -> Duration {
    let ceiling = policy
        .base
        .saturating_mul(2u32.saturating_pow(retry.min(16)))
        .min(policy.cap);
    let nanos = u64::try_from(ceiling.as_nanos()).unwrap_or(u64::MAX);
    if nanos == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos(random % (nanos + 1))
}

#[derive(Debug)]
struct Tokens {
    balance: f64,
    cap: f64,
}

#[derive(Debug, Clone)]
pub struct RetryBudget {
    ratio: f64,
    tokens: Arc<Mutex<Tokens>>,
}

impl RetryBudget {
    pub fn new(ratio: f64, cap: f64) -> RetryBudget {
        RetryBudget {
            ratio,
            tokens: Arc::new(Mutex::new(Tokens { balance: cap, cap })),
        }
    }

    pub fn deposit(&self) {
        let mut tokens = self.tokens.lock().unwrap_or_else(PoisonError::into_inner);
        tokens.balance = (tokens.balance + self.ratio).min(tokens.cap);
    }

    pub fn withdraw(&self) -> bool {
        let mut tokens = self.tokens.lock().unwrap_or_else(PoisonError::into_inner);
        if tokens.balance >= 1.0 {
            tokens.balance -= 1.0;
            true
        } else {
            false
        }
    }

    pub fn balance(&self) -> f64 {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .balance
    }
}

impl Default for RetryBudget {
    fn default() -> RetryBudget {
        RetryBudget::new(0.1, 10.0)
    }
}

fn random() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(0x5eed);
    hasher.finish()
}

pub async fn retry<T, E, F, Fut>(
    policy: &RetryPolicy,
    budget: &RetryBudget,
    deadline: Option<Deadline>,
    retryable: impl Fn(&E) -> bool,
    mut op: F,
) -> Result<T, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    budget.deposit();
    let mut attempt = 0;
    loop {
        let error = match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        attempt += 1;
        if attempt >= policy.attempts.max(1) || !retryable(&error) || !budget.withdraw() {
            return Err(error);
        }
        let pause = full_jitter(policy, attempt - 1, random());
        if deadline.is_some_and(|deadline| deadline.left() <= pause) {
            return Err(error);
        }
        sleep(pause).await;
    }
}
