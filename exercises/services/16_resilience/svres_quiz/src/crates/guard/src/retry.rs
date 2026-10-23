use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Unavailable,
    Aborted,
    Internal,
    Unknown,
    DeadlineExceeded,
    ResourceExhausted,
    InvalidArgument,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    Unauthenticated,
    FailedPrecondition,
    Cancelled,
    Unimplemented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status(pub Code);

impl Code {
    /// Виновата ли зависимость: ошибки клиента не считаются её неудачей.
    pub fn blames_dependency(self) -> bool {
        matches!(self, Code::Unavailable | Code::Aborted | Code::Internal | Code::Unknown | Code::DeadlineExceeded | Code::ResourceExhausted)
    }
}

pub struct Attempt {
    pub code: Code,
    pub idempotent: bool,
    pub budget_left: Duration,
    pub pause: Duration,
}

/// Повторять ли: нужно время; `Unavailable` всегда; `Aborted`, `Internal`, `Unknown`, `DeadlineExceeded` только идемпотентное.
pub fn should_retry(a: &Attempt) -> bool {
    if a.budget_left <= a.pause {
        return false;
    }
    match a.code {
        Code::Unavailable => true,
        Code::Aborted | Code::Internal | Code::Unknown | Code::DeadlineExceeded => a.idempotent,
        _ => false,
    }
}

/// Источник случайности: число из [0, 1).
pub trait Jitter {
    fn unit(&mut self) -> f64;
}

pub struct Lcg(pub u64);

impl Jitter for Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Потолок паузы для попытки `attempt` (с нуля): `base * 2^attempt`, но не больше `cap`.
pub fn ceiling(attempt: u32, base: Duration, cap: Duration) -> Duration {
    base.saturating_mul(1u32 << attempt.min(31)).min(cap)
}

/// Пауза с полным джиттером: от нуля до потолка.
pub fn backoff(attempt: u32, base: Duration, cap: Duration, rng: &mut impl Jitter) -> Duration {
    ceiling(attempt, base, cap).mul_f64(rng.unit())
}

/// Бюджет повторов: каждый запрос даёт `ratio` кредита, каждый повтор тратит единицу, копится не больше `reserve`.
pub struct RetryBudget {
    ratio: f64,
    reserve: f64,
    balance: f64,
}

impl RetryBudget {
    pub fn new(ratio: f64, reserve: u32) -> RetryBudget {
        RetryBudget { ratio, reserve: reserve as f64, balance: reserve as f64 }
    }

    pub fn on_request(&mut self) {
        self.balance = (self.balance + self.ratio).min(self.reserve);
    }

    pub fn try_retry(&mut self) -> bool {
        if self.balance >= 1.0 {
            self.balance -= 1.0;
            true
        } else {
            false
        }
    }
}
