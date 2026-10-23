use crate::hist::LogHistogram;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub ok: u64,
    pub errors: u64,
    pub connections: u64,
    pub elapsed: Duration,
    pub rps: f64,
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
}

impl Report {
    pub fn from_parts(hist: &LogHistogram, errors: u64, connections: u64, elapsed: Duration) -> Report {
        let ok = hist.count();
        let rps = if elapsed.is_zero() { 0.0 } else { ok as f64 / elapsed.as_secs_f64() };
        Report { ok, errors, connections, elapsed, rps, p50: hist.value_at_quantile(0.5), p99: hist.value_at_quantile(0.99), max: hist.max() }
    }

    pub fn error_share(&self) -> f64 {
        let total = self.ok + self.errors;
        if total == 0 { 0.0 } else { self.errors as f64 / total as f64 }
    }

    pub fn to_line(&self) -> String {
        todo!()
    }

    pub fn from_line(line: &str) -> Option<Report> {
        let _ = (line,);
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    pub p99_growth: f64,
    pub rps_drop: f64,
    pub error_share_growth: f64,
}

impl Default for Tolerance {
    fn default() -> Tolerance {
        Tolerance { p99_growth: 0.2, rps_drop: 0.1, error_share_growth: 0.005 }
    }
}

pub fn compare(prev: &Report, now: &Report, tol: &Tolerance) -> Vec<String> {
    let _ = (prev, now, tol);
    todo!()
}
