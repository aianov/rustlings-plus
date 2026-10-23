use crate::report::Report;
use crate::run::{Target, run_open};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_p99: Duration,
    pub max_error_share: f64,
}

#[derive(Debug)]
pub struct StepsOutcome {
    pub steps: Vec<(u32, Report)>,
    pub sustainable: Option<u32>,
}

pub async fn run_steps(target: Arc<dyn Target>, rates: &[u32], step_len: Duration, limits: &Limits) -> StepsOutcome {
    let mut steps = Vec::new();
    let mut sustainable = None;
    for &rate in rates {
        let report = run_open(target.clone(), rate, step_len).await;
        let within = report.p99 <= limits.max_p99 && report.error_share() <= limits.max_error_share;
        steps.push((rate, report));
        if !within {
            break;
        }
        sustainable = Some(rate);
    }
    StepsOutcome { steps, sustainable }
}
