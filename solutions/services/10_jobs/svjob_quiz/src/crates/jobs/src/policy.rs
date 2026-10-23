#[derive(Debug, Clone, PartialEq)]
pub enum JobError {
    Temporary(String),
    Permanent(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Done,
    Retry { after_ms: u64 },
    Dead { reason: String },
}

pub const MAX_ATTEMPTS: u32 = 5;

pub fn backoff_ms(attempt: u32) -> u64 {
    let exp = attempt.saturating_sub(1).min(10);
    (1000u64 << exp).min(60_000)
}

pub fn decide(result: Result<(), JobError>, attempt: u32) -> Action {
    match result {
        Ok(()) => Action::Done,
        Err(JobError::Permanent(why)) => Action::Dead { reason: why },
        Err(JobError::Temporary(why)) if attempt >= MAX_ATTEMPTS => Action::Dead { reason: format!("попытки исчерпаны ({MAX_ATTEMPTS}): {why}") },
        Err(JobError::Temporary(_)) => Action::Retry { after_ms: backoff_ms(attempt) },
    }
}
