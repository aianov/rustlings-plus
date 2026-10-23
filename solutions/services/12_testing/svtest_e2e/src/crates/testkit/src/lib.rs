use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{Instant, sleep};

pub fn unique_prefix() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("e2e_{}_{nanos}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst))
}

pub async fn eventually<T, F, Fut>(within: Duration, every: Duration, mut probe: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let deadline = Instant::now() + within;
    loop {
        let reason = match probe().await {
            Ok(value) => return Ok(value),
            Err(reason) => reason,
        };
        let now = Instant::now();
        if now >= deadline {
            return Err(format!("не дождались за {within:?}: {reason}"));
        }
        sleep(every.min(deadline - now)).await;
    }
}
