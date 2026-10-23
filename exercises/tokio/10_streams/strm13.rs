// Записи в базу выгоднее делать пачками. Есть два режима. В первом нужны пачки строго по `size`
// (последняя может быть короче): грузим архив. Во втором пачка набирается из всего, что уже
// пришло, но не больше `max`, и ждать недостающего нельзя: пишем живой поток, где задержка
// важнее размера пачки.
// TODO: Допиши `fixed_batches` и `ready_batches`.
use futures::stream::{self, Stream, StreamExt};
use tokio::sync::mpsc;

fn rx_stream<T>(rx: mpsc::Receiver<T>) -> impl Stream<Item = T> {
    stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })
}

fn fixed_batches<S: Stream<Item = u32>>(items: S, size: usize) -> impl Stream<Item = Vec<u32>> {
    let _ = size;
    items.map(|x| vec![x])
}

fn ready_batches<S: Stream<Item = u32>>(items: S, max: usize) -> impl Stream<Item = Vec<u32>> {
    let _ = max;
    items.map(|x| vec![x])
}

#[tokio::main]
async fn main() {
    let sizes: Vec<usize> = fixed_batches(stream::iter(0..25), 10).map(|b| b.len()).collect().await;
    println!("{sizes:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, Instant};

    #[tokio::test]
    async fn fixed_batches_have_exact_size_except_the_last() {
        let sizes: Vec<usize> = fixed_batches(stream::iter(0..25), 10).map(|b| b.len()).collect().await;
        assert_eq!(sizes, vec![10, 10, 5], "25 элементов по 10: две полные пачки и остаток");
    }

    #[tokio::test(start_paused = true)]
    async fn ready_batches_do_not_wait_for_more() {
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            for n in 0..5 {
                tx.send(n).await.unwrap();
            }
            sleep(Duration::from_secs(1)).await;
            for n in 5..8 {
                tx.send(n).await.unwrap();
            }
        });
        let start = Instant::now();
        let mut batches = Box::pin(ready_batches(rx_stream(rx), 10));
        sleep(Duration::from_millis(1)).await;
        let first = batches.next().await.unwrap();
        assert_eq!(first, vec![0, 1, 2, 3, 4], "первая пачка это всё, что уже пришло");
        assert!(start.elapsed() < Duration::from_millis(100), "первую пачку нельзя задерживать в ожидании остальных: прошло {:?}", start.elapsed());
        let second = batches.next().await.unwrap();
        assert_eq!(second, vec![5, 6, 7], "вторая пачка это поздние элементы");
        assert!(batches.next().await.is_none(), "потом конец");
    }

    #[tokio::test]
    async fn ready_batches_respect_the_maximum() {
        let sizes: Vec<usize> = ready_batches(stream::iter(0..25), 10).map(|b| b.len()).collect().await;
        assert_eq!(sizes, vec![10, 10, 5], "готовых элементов 25, но пачка не больше 10");
    }
}
