use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

async fn slow_square(n: u64) -> u64 {
    tokio::time::sleep(Duration::from_millis(50)).await;
    n * n
}

async fn squares(items: Vec<u64>) -> Vec<u64> {
    let jobs: Vec<_> = items.into_iter().map(|n| tokio::spawn(slow_square(n))).collect();
    let mut out = Vec::new();
    for job in jobs {
        out.push(job.await.unwrap());
    }
    out
}

fn notify_in_background(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        flag.store(true, Ordering::SeqCst);
    });
}

#[tokio::main]
async fn main() {
    println!("{:?}", squares(vec![1, 2, 3]).await);
    let flag = Arc::new(AtomicBool::new(false));
    notify_in_background(Arc::clone(&flag));
    tokio::time::sleep(Duration::from_millis(20)).await;
    println!("флаг: {}", flag.load(Ordering::SeqCst));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn tasks_run_concurrently() {
        let started = Instant::now();
        let result = squares(vec![1, 2, 3]).await;
        assert_eq!(result, [1, 4, 9], "результаты идут в порядке входа");
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "три задачи по 50 мс заняли {:?}: они работали по очереди. Future сам не запускается, нужен spawn до ожидания",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn background_task_runs_without_being_awaited() {
        let flag = Arc::new(AtomicBool::new(false));
        notify_in_background(Arc::clone(&flag));
        assert!(!flag.load(Ordering::SeqCst), "через 0 мс флаг ещё не должен стоять");
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(flag.load(Ordering::SeqCst), "задача после spawn работает сама, даже если хэндл никто не ждёт");
    }
}
