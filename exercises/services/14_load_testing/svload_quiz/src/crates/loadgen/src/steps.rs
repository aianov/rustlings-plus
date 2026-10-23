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
    let _ = (target, rates, step_len, limits);
    todo!()
}
