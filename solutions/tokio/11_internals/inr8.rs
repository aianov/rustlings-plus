use tokio::runtime::{Builder, Runtime};

fn build_runtime(limit: usize) -> Runtime {
    Builder::new_multi_thread().worker_threads(2).max_blocking_threads(limit).enable_all().build().unwrap()
}

fn main() {
    let rt = build_runtime(4);
    let answer = rt.block_on(async { tokio::task::spawn_blocking(|| 6 * 7).await.unwrap() });
    println!("{answer}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn job(running: &Arc<AtomicUsize>, peak: &Arc<AtomicUsize>, done: &Arc<AtomicUsize>, pause: u64) -> impl FnOnce() + Send + 'static {
        let (running, peak, done) = (running.clone(), peak.clone(), done.clone());
        move || {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(pause));
            running.fetch_sub(1, Ordering::SeqCst);
            done.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn at_most_limit_jobs_run_and_all_of_them_finish() {
        let rt = build_runtime(3);
        let (running, peak, done) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        rt.block_on(async {
            let handles: Vec<_> = (0..9).map(|_| tokio::task::spawn_blocking(job(&running, &peak, &done, 80))).collect();
            for handle in handles {
                handle.await.unwrap();
            }
        });
        assert_eq!(done.load(Ordering::SeqCst), 9, "лишние задания должны ждать очереди, а не пропадать");
        assert_eq!(peak.load(Ordering::SeqCst), 3, "одновременно блокирующая работа должна идти ровно в трёх потоках, не больше и не меньше");
    }

    #[test]
    fn async_tasks_keep_running_while_the_pool_is_saturated() {
        let rt = build_runtime(1);
        let (running, peak, done) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let finished_before_tick = rt.block_on(async {
            let blocking: Vec<_> = (0..2).map(|_| tokio::task::spawn_blocking(job(&running, &peak, &done, 300))).collect();
            tokio::time::sleep(Duration::from_millis(30)).await;
            let seen = done.load(Ordering::SeqCst);
            for handle in blocking {
                handle.await.unwrap();
            }
            seen
        });
        assert_eq!(finished_before_tick, 0, "пока блокирующий пул занят, асинхронный таймер на 30 мс должен срабатывать вовремя");
        assert_eq!(done.load(Ordering::SeqCst), 2, "оба задания в итоге выполнены");
        assert_eq!(peak.load(Ordering::SeqCst), 1, "при лимите 1 задания идут строго по очереди");
    }
}
