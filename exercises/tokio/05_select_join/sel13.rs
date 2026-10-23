// Обработчик слушает поток событий и заодно ждёт разовую фоновую синхронизацию (она занимает 100 мс).
// Результат синхронизации надо запомнить, а работа продолжается, пока не закроется канал событий.
// Пока синхронизация идёт, всё хорошо, но как только она закончилась, обработчик падает с паникой
// на следующем же событии.
// TODO: Найди причину и почини `run`.
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

async fn background_sync() -> u32 {
    sleep(Duration::from_millis(100)).await;
    7
}

async fn run(mut events: mpsc::Receiver<u32>) -> (Vec<u32>, Option<u32>) {
    let sync = background_sync();
    tokio::pin!(sync);
    let mut seen = Vec::new();
    let mut synced = None;
    loop {
        tokio::select! {
            result = &mut sync => synced = Some(result),
            event = events.recv() => match event {
                Some(event) => seen.push(event),
                None => break,
            },
        }
    }
    (seen, synced)
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel(4);
    tx.send(1).await.unwrap();
    drop(tx);
    println!("{:?}", run(rx).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    async fn emit(tx: mpsc::Sender<u32>, script: Vec<(u64, u32)>, close_after: u64) {
        let mut now = 0;
        for (at, event) in script {
            sleep(ms(at - now)).await;
            now = at;
            tx.send(event).await.unwrap();
        }
        sleep(ms(close_after - now)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn events_keep_flowing_after_sync_is_done() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(emit(tx, vec![(50, 1), (150, 2), (250, 3)], 300));
        let (seen, synced) = run(rx).await;
        assert_eq!(seen, vec![1, 2, 3], "события после конца синхронизации тоже должны быть обработаны");
        assert_eq!(synced, Some(7), "результат синхронизации должен быть запомнен");
    }

    #[tokio::test(start_paused = true)]
    async fn channel_may_close_before_sync_is_done() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(emit(tx, vec![(10, 1)], 30));
        let (seen, synced) = run(rx).await;
        assert_eq!(seen, vec![1], "событие обработано");
        assert_eq!(synced, None, "канал закрылся на 30 мс, синхронизация (100 мс) не успела: результата нет");
    }

    #[tokio::test(start_paused = true)]
    async fn quiet_stream_after_sync() {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(emit(tx, vec![], 500));
        let start = tokio::time::Instant::now();
        let (seen, synced) = run(rx).await;
        assert!(seen.is_empty(), "событий не было");
        assert_eq!(synced, Some(7), "синхронизация завершилась в тишине");
        assert_eq!(start.elapsed(), ms(500), "работа идёт до закрытия канала, а не до конца синхронизации");
    }
}
