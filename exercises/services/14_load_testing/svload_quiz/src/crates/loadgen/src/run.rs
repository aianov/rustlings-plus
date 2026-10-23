use crate::HIST_BITS;
use crate::hist::LogHistogram;
use crate::report::Report;
use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep_until};

pub trait Target: Send + Sync + 'static {
    fn connect(&self) -> BoxFuture<'_, Result<u64, String>>;
    fn request(&self, conn: u64) -> BoxFuture<'_, Result<(), String>>;
}

pub async fn run_closed(target: Arc<dyn Target>, workers: usize, duration: Duration) -> Report {
    let _ = (target, workers, duration);
    todo!()
}

pub async fn run_open(target: Arc<dyn Target>, rate: u32, duration: Duration) -> Report {
    let _ = (target, rate, duration);
    todo!()
}
