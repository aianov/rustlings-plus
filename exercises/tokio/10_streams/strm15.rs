// Подписка запускает фоновую задачу, которая раз в 10 мс кладёт новое событие в канал, а
// потребитель читает канал как поток. Потребитель взял три события и ушёл, поток брошен.
// Но фоновая задача продолжает жить и крутиться вхолостую: утечка, которая копится с каждой
// подпиской. Задача держит копию `probe` (так тест видит, жива ли она). Найди, почему
// задача не замечает, что читать её события больше некому.
use futures::stream::{self, Stream};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

fn subscribe(probe: Arc<()>) -> impl Stream<Item = u32> {
    let (tx, rx) = mpsc::channel(4);
    tokio::spawn(async move {
        let _probe = probe;
        let mut n = 0;
        loop {
            n += 1;
            let _ = tx.send(n).await;
            sleep(Duration::from_millis(10)).await;
        }
    });
    stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|n| (n, rx)) })
}

#[tokio::main]
async fn main() {
    use futures::StreamExt;
    let first: Vec<u32> = subscribe(Arc::new(())).take(2).collect().await;
    println!("{first:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test(start_paused = true)]
    async fn producer_stops_when_stream_is_dropped() {
        let probe = Arc::new(());
        let first: Vec<u32> = subscribe(probe.clone()).take(3).collect().await;
        assert_eq!(first, vec![1, 2, 3], "потребитель получил первые три события");
        sleep(Duration::from_millis(200)).await;
        assert_eq!(Arc::strong_count(&probe), 1, "поток брошен, а фоновая задача жива: она не замечает, что читать некому");
    }

    #[tokio::test(start_paused = true)]
    async fn events_keep_flowing_while_someone_reads() {
        let probe = Arc::new(());
        let mut events = Box::pin(subscribe(probe.clone()));
        for expected in 1..=5 {
            assert_eq!(events.next().await, Some(expected), "события идут по порядку, пока поток жив");
        }
        assert_eq!(Arc::strong_count(&probe), 2, "пока поток жив, задача тоже жива");
    }
}
