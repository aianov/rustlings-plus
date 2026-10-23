use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;

fn square(n: u32) -> u64 {
    n as u64 * n as u64
}

fn render(n: u32, sq: u64) -> String {
    format!("#{n}:{sq}")
}

fn start(total: u32, produced: Arc<AtomicU32>) -> mpsc::Receiver<String> {
    let (num_tx, mut num_rx) = mpsc::channel::<u32>(1);
    let (sq_tx, mut sq_rx) = mpsc::channel::<(u32, u64)>(1);
    let (out_tx, out_rx) = mpsc::channel::<String>(1);

    tokio::spawn(async move {
        for n in 0..total {
            if num_tx.send(n).await.is_err() {
                break;
            }
            produced.fetch_add(1, Ordering::SeqCst);
        }
    });
    tokio::spawn(async move {
        while let Some(n) = num_rx.recv().await {
            if sq_tx.send((n, square(n))).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some((n, sq)) = sq_rx.recv().await {
            if out_tx.send(render(n, sq)).await.is_err() {
                break;
            }
        }
    });
    out_rx
}

#[tokio::main]
async fn main() {
    let mut out = start(3, Arc::new(AtomicU32::new(0)));
    while let Some(line) = out.recv().await {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn source_cannot_run_far_ahead_of_a_slow_reader() {
        let produced = Arc::new(AtomicU32::new(0));
        let mut out = start(1000, produced.clone());
        for read in 1..=5u32 {
            out.recv().await.expect("поток не должен кончаться, пока номера не вышли");
            sleep(Duration::from_millis(100)).await;
            let ahead = produced.load(Ordering::SeqCst) as i64 - read as i64;
            assert!(
                ahead <= 12,
                "после {read} прочитанных источник уже выдал {} номеров, на {ahead} вперёд: очереди между стадиями безразмерные?",
                produced.load(Ordering::SeqCst)
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn everything_arrives_in_order_and_the_stream_ends() {
        let mut out = start(50, Arc::new(AtomicU32::new(0)));
        let mut lines = Vec::new();
        while let Some(line) = out.recv().await {
            lines.push(line);
        }
        let expected: Vec<String> = (0..50).map(|n| render(n, square(n))).collect();
        assert_eq!(lines, expected, "строки должны прийти все, по порядку, и поток должен закончиться сам");
    }

    #[tokio::test(start_paused = true)]
    async fn reader_leaving_stops_the_whole_chain() {
        let produced = Arc::new(AtomicU32::new(0));
        let mut out = start(1_000_000, produced.clone());
        for _ in 0..3 {
            out.recv().await.unwrap();
        }
        drop(out);
        sleep(Duration::from_secs(1)).await;
        let after_leaving = produced.load(Ordering::SeqCst);
        sleep(Duration::from_secs(1)).await;
        assert_eq!(produced.load(Ordering::SeqCst), after_leaving, "читатель ушёл, а источник всё выдаёт номера");
        assert!(after_leaving < 1000, "источник должен был остановиться вскоре после ухода читателя, а он выдал {after_leaving}");
    }
}
