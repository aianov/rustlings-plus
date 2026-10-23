use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub run_at: u64,
}

impl Job {
    pub fn new(id: &str, kind: &str, payload: Value) -> Job {
        Job {
            id: id.to_string(),
            kind: kind.to_string(),
            payload,
            run_at: 0,
        }
    }

    pub fn at(self, run_at: u64) -> Job {
        Job { run_at, ..self }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Transient(String),
    Permanent(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Transient(reason) => write!(f, "временная ошибка: {reason}"),
            Failure::Permanent(reason) => write!(f, "постоянная ошибка: {reason}"),
        }
    }
}

impl std::error::Error for Failure {}

pub type Outcome = Result<(), Failure>;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}
