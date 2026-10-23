// Запись в базу по одной строке дорогая, пачкой намного дешевле. `batch_writer` читает
// сообщения из канала и записывает в `store` пачками. Пачка это всё, что уже лежит в очереди (но
// не больше `max`), и ждать, пока она наполнится, нельзя: сообщение, пришедшее одно, пишется одно.
// TODO: Допиши `batch_writer`: одна запись в `store` на пачку, пустых пачек нет, конец наступает
// вместе с закрытием канала.
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Receiver;

async fn batch_writer(mut rx: Receiver<u32>, max: usize, store: Arc<Mutex<Vec<Vec<u32>>>>) {
    todo!()
}

#[tokio::main]
async fn main() {
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let store = Arc::new(Mutex::new(Vec::new()));
    for i in 0..5 {
        tx.send(i).await.unwrap();
    }
    drop(tx);
    batch_writer(rx, 3, Arc::clone(&store)).await;
    println!("{:?}", store.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn queued_messages_go_in_full_batches() {
        let (tx, rx) = mpsc::channel(16);
        for i in 0..10 {
            tx.send(i).await.unwrap();
        }
        drop(tx);
        let store = Arc::new(Mutex::new(Vec::new()));
        batch_writer(rx, 4, Arc::clone(&store)).await;
        assert_eq!(
            *store.lock().unwrap(),
            vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7], vec![8, 9]],
            "десять сообщений при max=4 это пачки по 4, 4 и 2, каждая пачка отдельной записью"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_wait_for_a_full_batch() {
        let (tx, rx) = mpsc::channel(16);
        let store = Arc::new(Mutex::new(Vec::new()));
        let writer = tokio::spawn(batch_writer(rx, 5, Arc::clone(&store)));
        for i in 0..3 {
            tx.send(i).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(tx);
        writer.await.unwrap();
        assert_eq!(
            *store.lock().unwrap(),
            vec![vec![0], vec![1], vec![2]],
            "сообщения приходят по одному раз в 10 мс: каждое должно записаться сразу, а не ждать остальные"
        );
    }

    #[tokio::test]
    async fn empty_channel_writes_nothing() {
        let (tx, rx) = mpsc::channel::<u32>(4);
        drop(tx);
        let store = Arc::new(Mutex::new(Vec::new()));
        batch_writer(rx, 4, Arc::clone(&store)).await;
        assert!(store.lock().unwrap().is_empty(), "пустых пачек быть не должно");
    }
}
