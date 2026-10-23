// Настройки (скажем, лимит длины поста) лежат в watch-канале, а воркер хочет реагировать на
// каждое изменение. Воркер должен отреагировать на то значение, которое действует на момент его
// старта, и затем на каждое новое; значения, которые успели смениться, пока он был занят,
// пропускаются: ему важно последнее.
// TODO: Допиши `observe`: вернуть список значений, на которые отреагировал воркер, пока отправитель
// не закрыл канал. Значение не должно попадать в список дважды.
use tokio::sync::watch;

async fn observe(mut rx: watch::Receiver<u32>) -> Vec<u32> {
    todo!()
}

#[tokio::main]
async fn main() {
    let (tx, rx) = watch::channel(10);
    let worker = tokio::spawn(observe(rx));
    tx.send(20).unwrap();
    drop(tx);
    println!("{:?}", worker.await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn reacts_to_changes_and_skips_intermediate() {
        let (tx, rx) = watch::channel(0);
        tx.send(1).unwrap();
        let seen = tokio::spawn(observe(rx));
        sleep(Duration::from_millis(10)).await;
        tx.send(2).unwrap();
        sleep(Duration::from_millis(10)).await;
        tx.send(3).unwrap();
        tx.send(4).unwrap();
        sleep(Duration::from_millis(10)).await;
        drop(tx);
        assert_eq!(
            seen.await.unwrap(),
            vec![1, 2, 4],
            "ждали [1, 2, 4]: стартовое значение 1 один раз, потом 2, потом из пары 3 и 4 только последнее"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn initial_value_counts_even_without_changes() {
        let (tx, rx) = watch::channel(5);
        let seen = tokio::spawn(observe(rx));
        sleep(Duration::from_millis(10)).await;
        tx.send(6).unwrap();
        sleep(Duration::from_millis(10)).await;
        drop(tx);
        assert_eq!(seen.await.unwrap(), vec![5, 6], "начальное значение 5 тоже действует, на него надо отреагировать");
    }

    #[tokio::test(start_paused = true)]
    async fn sender_gone_immediately() {
        let (tx, rx) = watch::channel(9);
        drop(tx);
        let seen = tokio::time::timeout(Duration::from_secs(1), observe(rx))
            .await
            .expect("observe завис: отправитель закрыт, ждать больше нечего");
        assert_eq!(seen, vec![9], "последнее значение остаётся читаемым и после закрытия");
    }
}
