use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

async fn collect_for(mut rx: mpsc::Receiver<u32>, window: Duration) -> Vec<u32> {
    let mut got = Vec::new();
    let deadline = sleep(window);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            score = rx.recv() => match score {
                Some(score) => got.push(score),
                None => break,
            },
            _ = &mut deadline => break,
        }
    }
    got
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(4);
    tx.send(5).await.unwrap();
    drop(tx);
    println!("{:?}", collect_for(rx, Duration::from_secs(1)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[tokio::test(start_paused = true)]
    async fn window_ends_even_while_messages_keep_coming() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            for score in 1..=10 {
                sleep(ms(100)).await;
                if tx.send(score).await.is_err() {
                    break;
                }
            }
        });
        let start = Instant::now();
        let got = collect_for(rx, ms(350)).await;
        assert_eq!(start.elapsed(), ms(350), "окно в 350 мс должно закончиться ровно в срок, а не когда иссякнут оценки");
        assert_eq!(got, vec![1, 2, 3], "за 350 мс успевают три оценки (на 100, 200 и 300 мс)");
    }

    #[tokio::test(start_paused = true)]
    async fn closed_channel_returns_at_once() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            sleep(ms(120)).await;
            tx.send(7).await.unwrap();
        });
        let start = Instant::now();
        let got = collect_for(rx, ms(5000)).await;
        assert_eq!(got, vec![7], "оценка пришла до конца окна");
        assert_eq!(start.elapsed(), ms(120), "канал закрылся на 120 мс, ждать конца окна незачем");
    }

    #[tokio::test(start_paused = true)]
    async fn silence_until_the_end_of_the_window() {
        let (tx, rx) = mpsc::channel::<u32>(8);
        let start = Instant::now();
        let got = collect_for(rx, ms(200)).await;
        drop(tx);
        assert!(got.is_empty(), "оценок не было");
        assert_eq!(start.elapsed(), ms(200), "тишина: ждём конец окна");
    }
}
