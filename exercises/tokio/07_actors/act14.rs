// Журнал событий пишется на диск пачками: до `max` штук, но не дольше `every` с момента первого
// события пачки. Сервис регулярно перезапускают, и после каждого перезапуска в журнале не хватает
// последних событий. Кроме того, ночью, когда события редки, пачки на диске выходят мельче, чем
// обещано: одно-два события вместо накопленных.
// TODO: Найди обе причины и почини `flush_loop`.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

struct Disk {
    writes: Mutex<Vec<Vec<u32>>>,
}

impl Disk {
    async fn write(&self, batch: Vec<u32>) {
        sleep(Duration::from_millis(20)).await;
        self.writes.lock().unwrap().push(batch);
    }
}

async fn flush_loop(mut rx: mpsc::Receiver<u32>, disk: Arc<Disk>, max: usize, every: Duration) {
    let mut buf = Vec::new();
    let mut tick = tokio::time::interval(every);
    loop {
        tokio::select! {
            item = rx.recv() => match item {
                Some(item) => {
                    buf.push(item);
                    if buf.len() >= max {
                        disk.write(std::mem::take(&mut buf)).await;
                    }
                }
                None => break,
            },
            _ = tick.tick() => {
                if !buf.is_empty() {
                    disk.write(std::mem::take(&mut buf)).await;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let disk = Arc::new(Disk { writes: Mutex::new(Vec::new()) });
    let (tx, rx) = mpsc::channel(8);
    let task = tokio::spawn(flush_loop(rx, disk.clone(), 3, Duration::from_millis(50)));
    tx.send(1).await.unwrap();
    drop(tx);
    task.await.unwrap();
    println!("{:?}", disk.writes.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn disk() -> Arc<Disk> {
        Arc::new(Disk { writes: Mutex::new(Vec::new()) })
    }

    #[tokio::test(start_paused = true)]
    async fn the_tail_survives_a_shutdown() {
        let disk = disk();
        let (tx, rx) = mpsc::channel(100);
        let task = tokio::spawn(flush_loop(rx, disk.clone(), 3, Duration::from_millis(100)));
        for n in 1..=5 {
            tx.send(n).await.unwrap();
        }
        drop(tx);
        task.await.unwrap();
        let writes = disk.writes.lock().unwrap().clone();
        assert_eq!(writes, vec![vec![1, 2, 3], vec![4, 5]], "после закрытия канала неполная пачка [4, 5] должна быть записана, а не потеряна");
    }

    #[tokio::test(start_paused = true)]
    async fn closing_flushes_at_once_and_never_writes_empty_batches() {
        let disk = disk();
        let (tx, rx) = mpsc::channel(100);
        let task = tokio::spawn(flush_loop(rx, disk.clone(), 3, Duration::from_secs(10)));
        for n in 1..=3 {
            tx.send(n).await.unwrap();
        }
        drop(tx);
        let start = Instant::now();
        task.await.unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "остановка не должна ждать срока every");
        let writes = disk.writes.lock().unwrap().clone();
        assert_eq!(writes, vec![vec![1, 2, 3]], "ровно одна полная пачка, пустых записей быть не должно");
    }

    #[tokio::test(start_paused = true)]
    async fn the_wait_is_counted_from_the_first_event_of_a_batch() {
        let disk = disk();
        let (tx, rx) = mpsc::channel(100);
        let task = tokio::spawn(flush_loop(rx, disk.clone(), 10, Duration::from_millis(100)));
        sleep(Duration::from_millis(90)).await;
        tx.send(1).await.unwrap();
        sleep(Duration::from_millis(40)).await;
        tx.send(2).await.unwrap();
        sleep(Duration::from_millis(40)).await;
        tx.send(3).await.unwrap();
        sleep(Duration::from_millis(200)).await;
        let writes = disk.writes.lock().unwrap().clone();
        assert_eq!(writes, vec![vec![1, 2, 3]], "пачка должна копиться до 100 мс после первого события, а не резаться по часам");
        drop(tx);
        task.await.unwrap();
    }
}
