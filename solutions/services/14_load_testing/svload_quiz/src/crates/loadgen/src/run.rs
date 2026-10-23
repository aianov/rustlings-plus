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
    let started = Instant::now();
    let deadline = started + duration;
    let mut set = JoinSet::new();
    for _ in 0..workers {
        let target = target.clone();
        set.spawn(async move {
            let mut hist = LogHistogram::new(HIST_BITS);
            let mut errors = 0u64;
            let Ok(conn) = target.connect().await else {
                return (hist, 1, 0);
            };
            while Instant::now() < deadline {
                let sent = Instant::now();
                match target.request(conn).await {
                    Ok(()) => hist.record(sent.elapsed()),
                    Err(_) => errors += 1,
                }
            }
            (hist, errors, 1)
        });
    }
    let mut total = LogHistogram::new(HIST_BITS);
    let (mut errors, mut connections) = (0, 0);
    while let Some(done) = set.join_next().await {
        if let Ok((hist, e, c)) = done {
            total.merge(&hist);
            errors += e;
            connections += c;
        }
    }
    Report::from_parts(&total, errors, connections, started.elapsed())
}

pub async fn run_open(target: Arc<dyn Target>, rate: u32, duration: Duration) -> Report {
    let started = Instant::now();
    let total_requests = (duration.as_nanos() * u128::from(rate) / 1_000_000_000) as u64;
    let idle: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let mut set = JoinSet::new();
    for i in 0..total_requests {
        let due = started + Duration::from_nanos((u128::from(i) * 1_000_000_000 / u128::from(rate)) as u64);
        sleep_until(due).await;
        let (target, idle) = (target.clone(), idle.clone());
        set.spawn(async move {
            let free = idle.lock().expect("набор не отравлен").pop();
            let (conn, opened) = match free {
                Some(conn) => (conn, false),
                None => match target.connect().await {
                    Ok(conn) => (conn, true),
                    Err(_) => return (None, false),
                },
            };
            let answer = target.request(conn).await;
            idle.lock().expect("набор не отравлен").push(conn);
            (answer.ok().map(|()| due.elapsed()), opened)
        });
    }
    let mut hist = LogHistogram::new(HIST_BITS);
    let (mut errors, mut connections) = (0, 0);
    while let Some(done) = set.join_next().await {
        let Ok((latency, opened)) = done else { continue };
        match latency {
            Some(latency) => hist.record(latency),
            None => errors += 1,
        }
        connections += u64::from(opened);
    }
    Report::from_parts(&hist, errors, connections, started.elapsed())
}
