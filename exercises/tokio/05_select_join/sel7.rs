// Уведомления копятся и уходят пачкой: не чаще одного раза в `every`, а не по штуке на каждое
// событие. Накопленное отправляется по таймеру, пустая пачка не отправляется. Когда источник
// закончился (канал закрыт), недосланное надо отправить сразу, не дожидаясь таймера.
// TODO: Допиши `batcher`: вернуть список отправленных пачек вместе с моментом отправки
// (сколько прошло с начала работы функции).
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, interval};

async fn batcher(mut rx: mpsc::Receiver<String>, every: Duration) -> Vec<(Duration, Vec<String>)> {
    todo!()
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(8);
    tx.send("лайк".to_string()).await.unwrap();
    drop(tx);
    println!("{:?}", batcher(rx, Duration::from_millis(100)).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    async fn feed(tx: mpsc::Sender<String>, script: Vec<(u64, &'static str)>, close_at: u64) {
        let start = Instant::now();
        for (at, text) in script {
            tokio::time::sleep_until(start + ms(at)).await;
            tx.send(text.to_string()).await.unwrap();
        }
        tokio::time::sleep_until(start + ms(close_at)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn flushes_on_timer_and_at_close() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(feed(tx, vec![(10, "a"), (20, "b"), (130, "c")], 140));
        let sent = batcher(rx, ms(100)).await;
        assert_eq!(
            sent,
            vec![(ms(100), vec!["a".to_string(), "b".to_string()]), (ms(140), vec!["c".to_string()])],
            "пачка [a, b] уходит по таймеру на 100 мс, остаток [c] при закрытии канала на 140 мс"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn empty_batches_are_not_sent() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(feed(tx, vec![(350, "late")], 360));
        let sent = batcher(rx, ms(100)).await;
        assert_eq!(sent, vec![(ms(360), vec!["late".to_string()])], "пустые пачки отправлять нельзя, а сообщение при закрытии канала уходит сразу");
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_at_all_is_nothing() {
        let (tx, rx) = mpsc::channel::<String>(8);
        drop(tx);
        let sent = batcher(rx, ms(100)).await;
        assert!(sent.is_empty(), "источник закрыт и ничего не прислал: отправлять нечего");
    }

    #[tokio::test(start_paused = true)]
    async fn one_batch_per_period_under_load() {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            for i in 0..30 {
                tx.send(format!("m{i}")).await.unwrap();
                sleep(ms(10)).await;
            }
        });
        let sent = batcher(rx, ms(100)).await;
        let total: usize = sent.iter().map(|(_, batch)| batch.len()).sum();
        assert_eq!(total, 30, "ни одно сообщение не должно пропасть");
        assert!(sent.len() <= 4, "30 сообщений за 300 мс при периоде 100 мс дают три-четыре пачки, а не {}", sent.len());
    }
}
