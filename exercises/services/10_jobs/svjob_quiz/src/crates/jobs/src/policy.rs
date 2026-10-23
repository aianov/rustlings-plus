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
    let _ = (attempt,);
    todo!()
}

pub fn decide(result: Result<(), JobError>, attempt: u32) -> Action {
    let _ = (result, attempt);
    todo!()
}
