use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use contracts::Username;
use service_runtime::Secret;
use tokio::time::{Instant, sleep};

use super::{AuthError, AuthService, Issued};

pub const THROTTLE_BASE: Duration = Duration::from_millis(100);

pub const THROTTLE_CAP: Duration = Duration::from_secs(5);

pub const THROTTLE_FORGET: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy)]
struct Strikes {
    count: u32,
    last: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThrottleKey {
    Account(String),
    Address(IpAddr),
}

#[derive(Debug)]
pub struct Throttle {
    base: Duration,
    cap: Duration,
    forget: Duration,
    strikes: Mutex<HashMap<ThrottleKey, Strikes>>,
}

impl Throttle {
    pub fn new(base: Duration, cap: Duration, forget: Duration) -> Throttle {
        Throttle {
            base,
            cap,
            forget,
            strikes: Mutex::new(HashMap::new()),
        }
    }

    fn delay_of(&self, strikes: Option<&Strikes>, now: Instant) -> Duration {
        let Some(strikes) = strikes.filter(|own| now.duration_since(own.last) < self.forget) else {
            return Duration::ZERO;
        };
        if strikes.count == 0 {
            return Duration::ZERO;
        }
        let doubled = self
            .base
            .saturating_mul(2u32.saturating_pow(strikes.count.saturating_sub(1).min(16)));
        doubled.min(self.cap)
    }

    pub fn after(&self, count: u32) -> Duration {
        if count == 0 {
            return Duration::ZERO;
        }
        let doubled = self
            .base
            .saturating_mul(2u32.saturating_pow(count.saturating_sub(1).min(16)));
        doubled.min(self.cap)
    }

    pub fn forget(&self) -> Duration {
        self.forget
    }

    pub fn delay(&self, keys: &[ThrottleKey]) -> Duration {
        let now = Instant::now();
        let strikes = self.strikes.lock().unwrap_or_else(PoisonError::into_inner);
        keys.iter()
            .map(|key| self.delay_of(strikes.get(key), now))
            .max()
            .unwrap_or(Duration::ZERO)
    }

    pub fn failed(&self, keys: &[ThrottleKey]) {
        let now = Instant::now();
        let mut strikes = self.strikes.lock().unwrap_or_else(PoisonError::into_inner);
        for key in keys {
            let entry = strikes.entry(key.clone()).or_insert(Strikes {
                count: 0,
                last: now,
            });
            if now.duration_since(entry.last) >= self.forget {
                entry.count = 0;
            }
            entry.count = entry.count.saturating_add(1);
            entry.last = now;
        }
        if strikes.len() > 100_000 {
            let forget = self.forget;
            strikes.retain(|_, own| now.duration_since(own.last) < forget);
        }
    }

    pub fn succeeded(&self, account: &str) {
        let mut strikes = self.strikes.lock().unwrap_or_else(PoisonError::into_inner);
        strikes.remove(&ThrottleKey::Account(account.to_string()));
    }
}

impl Default for Throttle {
    fn default() -> Throttle {
        Throttle::new(THROTTLE_BASE, THROTTLE_CAP, THROTTLE_FORGET)
    }
}

impl AuthService {
    pub fn with_throttle(self, throttle: Throttle) -> AuthService {
        AuthService { throttle, ..self }
    }

    pub async fn login_from(
        &self,
        name: &Username,
        password: &Secret<String>,
        address: Option<IpAddr>,
    ) -> Result<Issued, AuthError> {
        let mut keys = vec![ThrottleKey::Account(name.as_ref().to_string())];
        keys.extend(address.map(ThrottleKey::Address));
        let wait = match self.activity() {
            Some(activity) => self.throttle.after(activity.strikes(keys.clone()).await),
            None => self.throttle.delay(&keys),
        };
        if !wait.is_zero() {
            sleep(wait).await;
        }
        let result = self.login(name, password).await;
        match (&result, self.activity()) {
            (Ok(issued), Some(activity)) => {
                activity.cleared(name.as_ref().to_string()).await;
                self.session_opened(issued).await;
            }
            (Ok(..), None) => self.throttle.succeeded(name.as_ref()),
            (Err(AuthError::InvalidCredentials), Some(activity)) => activity.failed(keys).await,
            (Err(AuthError::InvalidCredentials), None) => self.throttle.failed(&keys),
            (Err(..), _) => {}
        }
        result
    }
}
