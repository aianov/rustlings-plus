use std::future::Future;
use std::time::Duration;

use crate::report::Report;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ramp {
    pub start: f64,
    pub factor: f64,
    pub steps: usize,
    pub p99: Duration,
    pub errors: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    Latency { rate: f64, p99_ms: f64 },
    Errors { rate: f64, error_rate: f64 },
    OutOfSteps,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RampOutcome {
    pub reports: Vec<(f64, Report)>,
    pub best: Option<(f64, Report)>,
    pub stop: Stop,
}

impl Default for Ramp {
    fn default() -> Ramp {
        Ramp {
            start: 10.0,
            factor: 2.0,
            steps: 8,
            p99: Duration::from_millis(250),
            errors: 0.01,
        }
    }
}

pub async fn ramp<F, Fut>(plan: Ramp, mut run: F) -> RampOutcome
where
    F: FnMut(f64) -> Fut,
    Fut: Future<Output = Report>,
{
    let mut reports = Vec::new();
    let mut best = None;
    let mut rate = plan.start.max(0.001);
    let limit_ms = plan.p99.as_secs_f64() * 1000.0;
    for _ in 0..plan.steps.max(1) {
        let report = run(rate).await;
        reports.push((rate, report.clone()));
        if report.error_rate > plan.errors {
            let stop = Stop::Errors {
                rate,
                error_rate: report.error_rate,
            };
            return RampOutcome {
                reports,
                best,
                stop,
            };
        }
        if report.p99_ms > limit_ms {
            let stop = Stop::Latency {
                rate,
                p99_ms: report.p99_ms,
            };
            return RampOutcome {
                reports,
                best,
                stop,
            };
        }
        best = Some((rate, report));
        rate *= plan.factor.max(1.01);
    }
    RampOutcome {
        reports,
        best,
        stop: Stop::OutOfSteps,
    }
}
