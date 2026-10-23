use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolClosed;

impl fmt::Display for PoolClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("пул закрыт и новых заданий не принимает")
    }
}

impl Error for PoolClosed {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobError {
    Panicked,
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobError::Panicked => f.write_str("задание упало с паникой и результата не оставило"),
        }
    }
}

impl Error for JobError {}
