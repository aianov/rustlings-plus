use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep_until};

use crate::histogram::Histogram;
use crate::report::{Report, StepReport, millis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    pub step: &'static str,
    pub took: Duration,
    pub ok: bool,
}

pub type Hit =
    Arc<dyn Fn(usize) -> Pin<Box<dyn Future<Output = Vec<Sample>> + Send>> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Closed {
    pub users: usize,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Open {
    pub rate: f64,
    pub duration: Duration,
    pub workers: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub total: Histogram,
    pub errors: u64,
    pub steps: BTreeMap<&'static str, (Histogram, u64)>,
    pub elapsed: Duration,
}

fn micros(took: Duration) -> u64 {
    u64::try_from(took.as_micros()).unwrap_or(u64::MAX)
}

impl Stats {
    pub fn add(&mut self, sample: &Sample) {
        let entry = self.steps.entry(sample.step).or_default();
        if sample.ok {
            self.total.record(micros(sample.took));
            entry.0.record(micros(sample.took));
        } else {
            self.errors += 1;
            entry.1 += 1;
        }
    }

    pub fn merge(&mut self, other: &Stats) {
        self.total.merge(&other.total);
        self.errors += other.errors;
        for (step, (histogram, errors)) in &other.steps {
            let entry = self.steps.entry(step).or_default();
            entry.0.merge(histogram);
            entry.1 += errors;
        }
    }

    pub fn report(&self, label: &str) -> Report {
        let steps = self
            .steps
            .iter()
            .map(|(name, (histogram, errors))| StepReport {
                name: (*name).to_string(),
                requests: histogram.count() + errors,
                errors: *errors,
                p50_ms: millis(histogram.quantile(0.5)),
                p99_ms: millis(histogram.quantile(0.99)),
            })
            .collect();
        Report::build(
            label,
            self.elapsed.as_secs_f64(),
            &self.total,
            self.errors,
            steps,
        )
    }
}

pub async fn run_closed(model: Closed, hit: Hit) -> Stats {
    let started = Instant::now();
    let deadline = started + model.duration;
    let mut tasks = JoinSet::new();
    for worker in 0..model.users.max(1) {
        let hit = Arc::clone(&hit);
        tasks.spawn(async move {
            let mut own = Stats::default();
            while Instant::now() < deadline {
                for sample in hit(worker).await {
                    own.add(&sample);
                }
            }
            own
        });
    }
    let mut stats = Stats::default();
    while let Some(done) = tasks.join_next().await {
        if let Ok(own) = done {
            stats.merge(&own);
        }
    }
    stats.elapsed = started.elapsed();
    stats
}

pub async fn run_open(model: Open, hit: Hit) -> Stats {
    let started = Instant::now();
    let workers = model.workers.max(1);
    let free = Arc::new(Mutex::new((0..workers).collect::<Vec<usize>>()));
    let permits = Arc::new(Semaphore::new(workers));
    let gap = Duration::from_secs_f64(1.0 / model.rate.max(0.001));
    let mut tasks = JoinSet::new();
    let mut arrival = 0u32;
    loop {
        let scheduled = started + gap * arrival;
        if scheduled >= started + model.duration {
            break;
        }
        arrival += 1;
        sleep_until(scheduled).await;
        let hit = Arc::clone(&hit);
        let free = Arc::clone(&free);
        let permits = Arc::clone(&permits);
        tasks.spawn(async move {
            let Ok(_permit) = permits.acquire().await else {
                return Vec::new();
            };
            let waited = scheduled.elapsed();
            let worker = free
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop()
                .unwrap_or(0);
            let mut samples = hit(worker).await;
            free.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(worker);
            if let Some(first) = samples.first_mut() {
                first.took += waited;
            }
            samples
        });
    }
    let mut stats = Stats::default();
    while let Some(done) = tasks.join_next().await {
        for sample in done.unwrap_or_default() {
            stats.add(&sample);
        }
    }
    stats.elapsed = started.elapsed();
    stats
}
