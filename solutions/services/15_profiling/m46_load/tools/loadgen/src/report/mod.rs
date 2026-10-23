use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::histogram::Histogram;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepReport {
    pub name: String,
    pub requests: u64,
    pub errors: u64,
    pub p50_ms: f64,
    pub p99_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub label: String,
    pub seconds: f64,
    pub requests: u64,
    pub errors: u64,
    pub error_rate: f64,
    pub rps: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub steps: Vec<StepReport>,
}

pub fn millis(micros: u64) -> f64 {
    micros as f64 / 1000.0
}

impl Report {
    pub fn build(
        label: &str,
        seconds: f64,
        total: &Histogram,
        errors: u64,
        steps: Vec<StepReport>,
    ) -> Report {
        let requests = total.count() + errors;
        Report {
            label: label.to_string(),
            seconds,
            requests,
            errors,
            error_rate: if requests == 0 {
                0.0
            } else {
                errors as f64 / requests as f64
            },
            rps: if seconds > 0.0 {
                requests as f64 / seconds
            } else {
                0.0
            },
            p50_ms: millis(total.quantile(0.50)),
            p90_ms: millis(total.quantile(0.90)),
            p99_ms: millis(total.quantile(0.99)),
            max_ms: millis(total.max()),
            steps,
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{}: {} запросов за {:.1} с, {:.1} в секунду, ошибок {} ({:.2}%)",
            self.label,
            self.requests,
            self.seconds,
            self.rps,
            self.errors,
            self.error_rate * 100.0
        );
        let _ = writeln!(
            out,
            "  p50 {:.1} мс, p90 {:.1} мс, p99 {:.1} мс, максимум {:.1} мс",
            self.p50_ms, self.p90_ms, self.p99_ms, self.max_ms
        );
        for step in &self.steps {
            let _ = writeln!(
                out,
                "  {:<8} {:>7} запросов, ошибок {:>4}, p50 {:>7.1} мс, p99 {:>7.1} мс",
                step.name, step.requests, step.errors, step.p50_ms, step.p99_ms
            );
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Report, String> {
        serde_json::from_str(text).map_err(|error| error.to_string())
    }
}

fn change(before: f64, after: f64) -> f64 {
    if before == 0.0 {
        return 0.0;
    }
    (after - before) / before * 100.0
}

pub fn compare(before: &Report, after: &Report) -> String {
    let rps = change(before.rps, after.rps);
    let p99 = change(before.p99_ms, after.p99_ms);
    let errors = after.error_rate - before.error_rate;
    let verdict = if p99 > 10.0 || rps < -10.0 || errors > 0.001 {
        "хуже прошлого прогона"
    } else if p99 < -10.0 || rps > 10.0 {
        "лучше прошлого прогона"
    } else {
        "в пределах шума"
    };
    format!(
        "сравнение с «{}»: в секунду {:+.1}%, p99 {:+.1}% ({:.1} → {:.1} мс), доля ошибок {:+.2} п.п.: {verdict}",
        before.label,
        rps,
        p99,
        before.p99_ms,
        after.p99_ms,
        errors * 100.0
    )
}
