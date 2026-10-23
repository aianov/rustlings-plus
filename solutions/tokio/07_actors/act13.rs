use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout_at};

async fn batcher(mut input: mpsc::Receiver<u32>, output: mpsc::Sender<Vec<u32>>, max: usize, wait: Duration) {
    let mut batch: Vec<u32> = Vec::new();
    let mut deadline: Option<Instant> = None;
    loop {
        let next = match deadline {
            None => input.recv().await,
            Some(at) => match timeout_at(at, input.recv()).await {
                Ok(item) => item,
                Err(_) => {
                    if output.send(std::mem::take(&mut batch)).await.is_err() {
                        return;
                    }
                    deadline = None;
                    continue;
                }
            },
        };
        match next {
            Some(item) => {
                if batch.is_empty() {
                    deadline = Some(Instant::now() + wait);
                }
                batch.push(item);
                if batch.len() == max {
                    if output.send(std::mem::take(&mut batch)).await.is_err() {
                        return;
                    }
                    deadline = None;
                }
            }
            None => {
                if !batch.is_empty() {
                    let _ = output.send(batch).await;
                }
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(8);
    let (out_tx, mut out_rx) = mpsc::channel(8);
    tokio::spawn(batcher(rx, out_tx, 3, Duration::from_millis(50)));
    for n in 0..7 {
        tx.send(n).await.unwrap();
    }
    drop(tx);
    while let Some(batch) = out_rx.recv().await {
        println!("{batch:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    fn setup(max: usize, wait_ms: u64) -> (mpsc::Sender<u32>, mpsc::Receiver<Vec<u32>>, Instant) {
        let (tx, rx) = mpsc::channel(100);
        let (out_tx, out_rx) = mpsc::channel(100);
        tokio::spawn(batcher(rx, out_tx, max, Duration::from_millis(wait_ms)));
        (tx, out_rx, Instant::now())
    }

    #[tokio::test(start_paused = true)]
    async fn a_full_batch_leaves_at_once() {
        let (tx, mut out, start) = setup(3, 100);
        for n in 1..=3 {
            tx.send(n).await.unwrap();
        }
        assert_eq!(out.recv().await, Some(vec![1, 2, 3]), "набралось max значений, пачка уходит сразу");
        assert_eq!(start.elapsed(), Duration::ZERO, "ждать таймер для полной пачки не нужно");
    }

    #[tokio::test(start_paused = true)]
    async fn a_partial_batch_leaves_after_wait() {
        let (tx, mut out, start) = setup(3, 100);
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        assert_eq!(out.recv().await, Some(vec![1, 2]), "неполная пачка уходит по истечении wait");
        assert_eq!(start.elapsed(), Duration::from_millis(100), "ровно через wait после первого значения");
    }

    #[tokio::test(start_paused = true)]
    async fn the_timer_starts_at_the_first_value_of_each_batch() {
        let (tx, mut out, start) = setup(3, 100);
        tx.send(1).await.unwrap();
        sleep(Duration::from_millis(60)).await;
        tx.send(2).await.unwrap();
        assert_eq!(out.recv().await, Some(vec![1, 2]), "второе значение пришло в пределах срока первого");
        assert_eq!(start.elapsed(), Duration::from_millis(100), "срок считается от первого значения, а не от последнего");
        sleep(Duration::from_millis(20)).await;
        tx.send(3).await.unwrap();
        assert_eq!(out.recv().await, Some(vec![3]), "новая пачка началась со значения 3");
        assert_eq!(start.elapsed(), Duration::from_millis(220), "у новой пачки свой отсчёт: 120 + 100 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn size_beats_time_and_empty_batches_never_happen() {
        let (tx, mut out, start) = setup(3, 100);
        for n in 1..=7 {
            tx.send(n).await.unwrap();
        }
        assert_eq!(out.recv().await, Some(vec![1, 2, 3]), "первая полная пачка");
        assert_eq!(out.recv().await, Some(vec![4, 5, 6]), "вторая полная пачка");
        assert_eq!(start.elapsed(), Duration::ZERO, "обе полные пачки ушли без ожидания");
        assert_eq!(out.recv().await, Some(vec![7]), "остаток уходит по сроку");
        assert_eq!(start.elapsed(), Duration::from_millis(100), "остаток ждал свой срок");
        sleep(Duration::from_millis(500)).await;
        assert!(out.try_recv().is_err(), "тихая пауза не должна рождать пустых пачек");
    }
}
