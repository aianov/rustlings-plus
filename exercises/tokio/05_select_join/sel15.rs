// Датчик присылает числа, которые надо разбирать парами: первое и второе вместе. Читатель пар
// работает в цикле с «тиками» (по ним, скажем, обновляется индикатор), и тик может случиться
// между первым числом пары и вторым. Тесты показывают, что после такого тика пары сдвигаются:
// число, прочитанное до тика, куда-то пропало.
// TODO: Почини `PairReader::next`: тик не должен ломать пары.
use tokio::sync::mpsc;

struct PairReader {
    rx: mpsc::Receiver<u32>,
    half: Option<u32>,
}

impl PairReader {
    fn new(rx: mpsc::Receiver<u32>) -> PairReader {
        PairReader { rx, half: None }
    }
}

async fn pump(mut reader: PairReader, mut ticks: mpsc::Receiver<()>) -> (Vec<(u32, u32)>, usize) {
    let mut pairs = Vec::new();
    let mut tick_count = 0;
    loop {
        tokio::select! {
            pair = reader.next() => match pair {
                Some(pair) => pairs.push(pair),
                None => break,
            },
            Some(()) = ticks.recv() => tick_count += 1,
        }
    }
    (pairs, tick_count)
}

impl PairReader {
    async fn next(&mut self) -> Option<(u32, u32)> {
        let first = self.rx.recv().await?;
        let second = self.rx.recv().await?;
        Some((first, second))
    }
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(4);
    let (_ticks_tx, ticks) = mpsc::channel(4);
    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();
    drop(tx);
    println!("{:?}", pump(PairReader::new(rx), ticks).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{Instant, sleep_until};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[tokio::test(start_paused = true)]
    async fn tick_between_the_halves_changes_nothing() {
        let (tx, rx) = mpsc::channel(8);
        let (ticks_tx, ticks) = mpsc::channel(8);
        let start = Instant::now();
        tokio::spawn(async move {
            for (at, n) in [(10, 1), (30, 2), (40, 3), (50, 4)] {
                sleep_until(start + ms(at)).await;
                tx.send(n).await.unwrap();
            }
        });
        tokio::spawn(async move {
            sleep_until(start + ms(20)).await;
            ticks_tx.send(()).await.unwrap();
            sleep_until(start + ms(1000)).await;
        });
        let (pairs, tick_count) = pump(PairReader::new(rx), ticks).await;
        assert_eq!(pairs, vec![(1, 2), (3, 4)], "пары должны быть (1, 2) и (3, 4): тик на 20 мс не вправе съесть единицу");
        assert_eq!(tick_count, 1, "тик один и должен быть учтён");
    }

    #[tokio::test(start_paused = true)]
    async fn many_ticks_in_a_row() {
        let (tx, rx) = mpsc::channel(8);
        let (ticks_tx, ticks) = mpsc::channel(64);
        let start = Instant::now();
        tokio::spawn(async move {
            for n in 0..6u32 {
                sleep_until(start + ms(100 * (u64::from(n) + 1))).await;
                tx.send(n).await.unwrap();
            }
        });
        tokio::spawn(async move {
            for i in 1..=60 {
                sleep_until(start + ms(10 * i)).await;
                let _ = ticks_tx.send(()).await;
            }
            sleep_until(start + ms(5000)).await;
        });
        let (pairs, tick_count) = pump(PairReader::new(rx), ticks).await;
        assert_eq!(pairs, vec![(0, 1), (2, 3), (4, 5)], "тики каждые 10 мс не должны мешать собирать пары");
        assert!(tick_count >= 55, "тики должны учитываться: {tick_count}");
    }

    #[tokio::test(start_paused = true)]
    async fn odd_tail_is_dropped_on_close() {
        let (tx, rx) = mpsc::channel(8);
        let (_ticks_tx, ticks) = mpsc::channel(8);
        for n in [1, 2, 3] {
            tx.send(n).await.unwrap();
        }
        drop(tx);
        let (pairs, _) = pump(PairReader::new(rx), ticks).await;
        assert_eq!(pairs, vec![(1, 2)], "тройка чисел даёт одну пару, одинокая тройка отбрасывается при закрытии канала");
    }
}
