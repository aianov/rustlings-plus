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
        json!({
            "ok": self.ok,
            "errors": self.errors,
            "connections": self.connections,
            "elapsed_us": self.elapsed.as_micros() as u64,
            "rps": self.rps,
            "p50_us": self.p50.as_micros() as u64,
            "p99_us": self.p99.as_micros() as u64,
            "max_us": self.max.as_micros() as u64,
        })
        .to_string()
    }

    pub fn from_line(line: &str) -> Option<Report> {
        let v: Value = serde_json::from_str(line).ok()?;
        let n = |key: &str| v.get(key).and_then(Value::as_u64);
        Some(Report {
            ok: n("ok")?,
            errors: n("errors")?,
            connections: n("connections")?,
            elapsed: Duration::from_micros(n("elapsed_us")?),
            rps: v.get("rps").and_then(Value::as_f64)?,
            p50: Duration::from_micros(n("p50_us")?),
            p99: Duration::from_micros(n("p99_us")?),
            max: Duration::from_micros(n("max_us")?),
        })
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
    let mut regressions = Vec::new();
    let p99_limit = prev.p99.mul_f64(1.0 + tol.p99_growth);
    if now.p99 > p99_limit {
        regressions.push(format!("p99 вырос: {:?} → {:?} (допустимо до {:?})", prev.p99, now.p99, p99_limit));
    }
    let rps_floor = prev.rps * (1.0 - tol.rps_drop);
    if now.rps < rps_floor {
        regressions.push(format!("скорость упала: {:.1} → {:.1} запросов в секунду (допустимо от {:.1})", prev.rps, now.rps, rps_floor));
    }
    if now.error_share() > prev.error_share() + tol.error_share_growth {
        regressions.push(format!("доля ошибок выросла: {:.3} → {:.3}", prev.error_share(), now.error_share()));
    }
    regressions
}
