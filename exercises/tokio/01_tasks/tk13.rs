// Параллельные задания, и если одно упало, остальные продолжать нет смысла. Сейчас код
// держит хэндлы в Vec: ждёт их по порядку, остальные задачи после ошибки продолжают работать
// впустую (дроп JoinHandle их не останавливает), а ошибку мы узнаём только когда дойдём до
// упавшей.
// TODO: Перепиши run_all: первая по времени ошибка возвращается сразу, остальные задания
// отменяются и не доходят до конца.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinSet;

type Job = (u64, bool);

async fn run_job(job: Job, finished: Arc<AtomicUsize>) -> Result<u64, String> {
    let (ms, succeeds) = job;
    tokio::time::sleep(Duration::from_millis(ms)).await;
    finished.fetch_add(1, Ordering::SeqCst);
    if succeeds { Ok(ms) } else { Err(format!("задание на {ms} мс упало")) }
}

async fn run_all(jobs: Vec<Job>, finished: Arc<AtomicUsize>) -> Result<usize, String> {
    let handles: Vec<_> = jobs
        .into_iter()
        .map(|job| tokio::spawn(run_job(job, Arc::clone(&finished))))
        .collect();
    let mut done = 0;
    for handle in handles {
        handle.await.unwrap()?;
        done += 1;
    }
    Ok(done)
}

#[tokio::main]
async fn main() {
    let finished = Arc::new(AtomicUsize::new(0));
    println!("{:?}", run_all(vec![(10, true), (20, false), (500, true)], finished).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn all_succeed() {
        let finished = Arc::new(AtomicUsize::new(0));
        let result = run_all(vec![(10, true), (20, true), (30, true)], Arc::clone(&finished)).await;
        assert_eq!(result, Ok(3), "три успеха: ответ 3");
    }

    #[tokio::test(start_paused = true)]
    async fn first_error_by_time_returns_at_once() {
        let finished = Arc::new(AtomicUsize::new(0));
        let started = Instant::now();
        let result = run_all(vec![(100, true), (10, false), (100, true)], Arc::clone(&finished)).await;
        assert_eq!(result, Err("задание на 10 мс упало".to_string()), "ошибка должна прийти первой по времени");
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "ошибка пришла через {:?}: нужно реагировать на упавшее задание сразу, а не ждать остальные по порядку",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn others_are_cancelled() {
        let finished = Arc::new(AtomicUsize::new(0));
        let _ = run_all(vec![(100, true), (10, false), (100, true)], Arc::clone(&finished)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            finished.load(Ordering::SeqCst),
            1,
            "после первой ошибки остальные задания должны быть отменены и не дойти до конца"
        );
    }
}
