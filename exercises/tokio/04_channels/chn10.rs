// Медленный подписчик не успевает читать broadcast, а буфер канала кольцевой: новые сообщения
// затирают старые. Тогда recv возвращает ошибку Lagged(n), где n это число пропущенных сообщений,
// и читать после неё можно дальше.
// TODO: Допиши `drain`: прочитай все сообщения до закрытия канала, после каждого подожди `delay`
// (изображает медленную обработку). Верни прочитанное и общее число пропущенных сообщений. Потеря
// не должна обрывать чтение, и никакая потеря не должна остаться неучтённой.
use std::time::Duration;
use tokio::sync::broadcast::{self, Receiver, error::RecvError};

async fn drain(mut rx: Receiver<u32>, delay: Duration) -> (Vec<u32>, u64) {
    let mut got = Vec::new();
    while let Ok(value) = rx.recv().await {
        got.push(value);
        tokio::time::sleep(delay).await;
    }
    (got, 0)
}

#[tokio::main]
async fn main() {
    let (tx, rx) = broadcast::channel(2);
    for i in 0..5 {
        tx.send(i).unwrap();
    }
    drop(tx);
    println!("{:?}", drain(rx, Duration::ZERO).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn burst_overwrites_the_oldest() {
        let (tx, rx) = broadcast::channel(4);
        for i in 0..10 {
            tx.send(i).unwrap();
        }
        drop(tx);
        let (got, missed) = drain(rx, Duration::ZERO).await;
        assert_eq!(got, vec![6, 7, 8, 9], "в буфере на четыре места остались последние четыре сообщения");
        assert_eq!(missed, 6, "шесть первых затёрты, и об этом надо знать");
    }

    #[tokio::test]
    async fn no_lag_no_loss() {
        let (tx, rx) = broadcast::channel(8);
        for i in 0..5 {
            tx.send(i).unwrap();
        }
        drop(tx);
        let (got, missed) = drain(rx, Duration::ZERO).await;
        assert_eq!(got, vec![0, 1, 2, 3, 4], "буфер вместил всё");
        assert_eq!(missed, 0, "потерь нет");
    }

    #[tokio::test(start_paused = true)]
    async fn slow_consumer_accounts_for_every_message() {
        let (tx, rx) = broadcast::channel(4);
        let consumer = tokio::spawn(drain(rx, Duration::from_millis(10)));
        for i in 0..20 {
            tx.send(i).unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        drop(tx);
        let (got, missed) = consumer.await.unwrap();
        assert!(missed > 0, "потребитель в десять раз медленнее производителя: потери обязаны случиться");
        assert_eq!(got.len() as u64 + missed, 20, "прочитанные и пропущенные вместе должны дать все 20 сообщений");
        assert!(got.windows(2).all(|w| w[0] < w[1]), "порядок прочитанных не нарушается: {got:?}");
        assert_eq!(got.last(), Some(&19), "самое свежее сообщение доходит всегда");
    }
}
