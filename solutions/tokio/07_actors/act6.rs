use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

async fn resize(ms: u64) -> u64 {
    sleep(Duration::from_millis(ms)).await;
    ms
}

async fn run_pool(workers: usize, jobs: Vec<u64>) -> Vec<(usize, u64)> {
    let (tx, rx) = mpsc::channel(jobs.len().max(1));
    for job in jobs {
        tx.send(job).await.unwrap();
    }
    drop(tx);

    let queue = Arc::new(tokio::sync::Mutex::new(rx));
    let done = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for id in 0..workers {
        let queue = queue.clone();
        let done = done.clone();
        handles.push(tokio::spawn(async move {
            loop {
                let next = queue.lock().await.recv().await;
                let Some(job) = next else { break };
                let result = resize(job).await;
                done.lock().unwrap().push((id, result));
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let mut done = done.lock().unwrap();
    std::mem::take(&mut *done)
}

#[tokio::main]
async fn main() {
    println!("{:?}", run_pool(2, vec![10, 20, 30]).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn eight_jobs_on_four_workers_take_two_rounds() {
        let start = Instant::now();
        let done = run_pool(4, vec![100; 8]).await;
        assert_eq!(done.len(), 8, "все восемь заданий должны быть сделаны");
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(200),
            "четыре воркера по два задания: должно быть 200 мс; больше значит, что воркеры работают по очереди"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_job_does_not_block_the_others() {
        let start = Instant::now();
        let mut jobs = vec![1000];
        jobs.extend([100; 6]);
        let done = run_pool(3, jobs).await;
        assert_eq!(done.len(), 7, "семь заданий, семь результатов");
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(1000),
            "пока один воркер занят долгим заданием, двое других обязаны разобрать остальные: всего 1000 мс"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_lost_nothing_doubled() {
        let jobs: Vec<u64> = (1..=30).collect();
        let done = run_pool(5, jobs.clone()).await;
        let mut results: Vec<u64> = done.iter().map(|(_, r)| *r).collect();
        results.sort();
        assert_eq!(results, jobs, "каждое задание ровно один раз");
    }
}
