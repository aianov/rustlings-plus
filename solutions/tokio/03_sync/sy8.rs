use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Default)]
struct Alive {
    now: AtomicUsize,
    peak: AtomicUsize,
}

struct AliveGuard(Arc<Alive>);

impl AliveGuard {
    fn new(alive: &Arc<Alive>) -> Self {
        let now = alive.now.fetch_add(1, Ordering::SeqCst) + 1;
        alive.peak.fetch_max(now, Ordering::SeqCst);
        AliveGuard(Arc::clone(alive))
    }
}

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.now.fetch_sub(1, Ordering::SeqCst);
    }
}

fn job(n: u32, alive: &Arc<Alive>) -> impl Future<Output = u32> + Send + 'static {
    let guard = AliveGuard::new(alive);
    async move {
        let _guard = guard;
        tokio::time::sleep(Duration::from_millis(10)).await;
        n * 2
    }
}

async fn run_bounded(items: Vec<u32>, limit: usize, alive: Arc<Alive>) -> Vec<u32> {
    let permits = Arc::new(tokio::sync::Semaphore::new(limit));
    let mut handles = Vec::new();
    for n in items {
        let permit = Arc::clone(&permits).acquire_owned().await.unwrap();
        let work = job(n, &alive);
        handles.push(tokio::spawn(async move {
            let result = work.await;
            drop(permit);
            result
        }));
    }
    let mut out = Vec::new();
    for handle in handles {
        out.push(handle.await.unwrap());
    }
    out
}

#[tokio::main]
async fn main() {
    let alive = Arc::new(Alive::default());
    println!("{:?}", run_bounded(vec![1, 2, 3], 2, alive).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn results_in_input_order() {
        let alive = Arc::new(Alive::default());
        let out = run_bounded((0..10).collect(), 3, Arc::clone(&alive)).await;
        assert_eq!(out, (0..10).map(|n| n * 2).collect::<Vec<_>>(), "результаты по порядку входа");
    }

    #[tokio::test(start_paused = true)]
    async fn never_more_than_limit_tasks_exist() {
        let alive = Arc::new(Alive::default());
        run_bounded((0..50).collect(), 4, Arc::clone(&alive)).await;
        let peak = alive.peak.load(Ordering::SeqCst);
        assert!(peak <= 4, "одновременно существовало {peak} задач при лимите 4: разрешение нужно брать до создания задачи");
        assert_eq!(peak, 4, "лимит должен использоваться полностью");
        assert_eq!(alive.now.load(Ordering::SeqCst), 0, "после работы живых задач нет");
    }

    #[tokio::test(start_paused = true)]
    async fn slots_are_reused() {
        let alive = Arc::new(Alive::default());
        let started = Instant::now();
        run_bounded((0..12).collect(), 4, Arc::clone(&alive)).await;
        assert_eq!(started.elapsed(), Duration::from_millis(30), "12 задач по 10 мс при лимите 4: три волны");
    }
}
