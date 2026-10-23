use tokio::sync::{mpsc, oneshot};

async fn run_worker(mut jobs: mpsc::UnboundedReceiver<u32>, mut stop: oneshot::Receiver<()>) -> usize {
    let mut processed = 0;
    loop {
        tokio::select! {
            biased;
            _ = &mut stop => break,
            job = jobs.recv() => match job {
                Some(_) => processed += 1,
                None => break,
            },
        }
    }
    processed
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::unbounded_channel();
    let (_stop_tx, stop) = oneshot::channel();
    tx.send(1).unwrap();
    drop(tx);
    println!("{}", run_worker(rx, stop).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn processes_everything_without_a_signal() {
        let (tx, rx) = mpsc::unbounded_channel();
        let (_stop_tx, stop) = oneshot::channel();
        for job in 0..100 {
            tx.send(job).unwrap();
        }
        drop(tx);
        assert_eq!(run_worker(rx, stop).await, 100, "сигнала нет: надо обработать все 100 заданий и выйти, когда очередь закрыта");
    }

    #[tokio::test(start_paused = true)]
    async fn stop_wins_over_a_full_queue() {
        for attempt in 0..30 {
            let (tx, rx) = mpsc::unbounded_channel();
            let (stop_tx, stop) = oneshot::channel();
            for job in 0..1000 {
                tx.send(job).unwrap();
            }
            stop_tx.send(()).unwrap();
            let processed = run_worker(rx, stop).await;
            assert_eq!(
                processed, 0,
                "попытка {attempt}: сигнал остановки уже подан, а воркер обработал {processed} заданий; сигнал должен проверяться раньше очереди"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stop_arrives_in_the_middle() {
        let (tx, rx) = mpsc::unbounded_channel();
        let (stop_tx, stop) = oneshot::channel();
        let worker = tokio::spawn(run_worker(rx, stop));
        for job in 0..10 {
            tx.send(job).unwrap();
        }
        tokio::task::yield_now().await;
        stop_tx.send(()).unwrap();
        for job in 10..5000 {
            tx.send(job).unwrap();
        }
        let processed = worker.await.unwrap();
        assert!(processed < 5000, "после остановки воркер продолжил разбирать очередь: обработано {processed}");
    }
}
