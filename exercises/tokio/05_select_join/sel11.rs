// Сервис журнала делает снимок состояния: операция долгая (десять шагов по 100 мс). Пока она идёт,
// сообщения продолжают приходить, и их нужно записывать в журнал сразу по приходу, а не после
// снимка. Снимок при этом должен идти один раз с начала до конца, а не начинаться заново на
// каждом сообщении.
// TODO: Допиши `snapshot_and_log`: вернуть результат снимка, когда он готов; сообщения записывать
// как (время прихода, текст).
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};

struct Journal {
    snapshot_starts: AtomicUsize,
}

impl Journal {
    async fn snapshot(&self) -> u32 {
        self.snapshot_starts.fetch_add(1, Ordering::SeqCst);
        for _ in 0..10 {
            sleep(Duration::from_millis(100)).await;
        }
        42
    }
}

async fn snapshot_and_log(journal: &Journal, mut rx: mpsc::Receiver<String>, log: &mut Vec<(Duration, String)>) -> u32 {
    todo!()
}

#[tokio::main]
async fn main() {
    let journal = Journal { snapshot_starts: AtomicUsize::new(0) };
    let (_tx, rx) = mpsc::channel(4);
    let mut log = Vec::new();
    println!("{}", snapshot_and_log(&journal, rx, &mut log).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[tokio::test(start_paused = true)]
    async fn snapshot_runs_once_to_the_end() {
        let journal = Journal { snapshot_starts: AtomicUsize::new(0) };
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            for (gap, text) in [(150u64, "a"), (200, "b"), (200, "c")] {
                sleep(ms(gap)).await;
                tx.send(text.to_string()).await.unwrap();
            }
            sleep(ms(5000)).await;
        });
        let mut log = Vec::new();
        let start = Instant::now();
        let result = snapshot_and_log(&journal, rx, &mut log).await;
        assert_eq!(result, 42, "результат снимка должен вернуться наружу");
        assert_eq!(journal.snapshot_starts.load(Ordering::SeqCst), 1, "снимок должен начаться ровно один раз, а не заново на каждом сообщении");
        assert_eq!(start.elapsed(), ms(1000), "снимок идёт свои 1000 мс, сообщения его не сдвигают");
    }

    #[tokio::test(start_paused = true)]
    async fn messages_are_logged_on_arrival() {
        let journal = Journal { snapshot_starts: AtomicUsize::new(0) };
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            for (gap, text) in [(100u64, "a"), (200, "b"), (200, "c")] {
                sleep(ms(gap)).await;
                tx.send(text.to_string()).await.unwrap();
            }
            sleep(ms(5000)).await;
            drop(tx);
        });
        let mut log = Vec::new();
        snapshot_and_log(&journal, rx, &mut log).await;
        assert_eq!(
            log,
            vec![(ms(100), "a".to_string()), (ms(300), "b".to_string()), (ms(500), "c".to_string())],
            "сообщения должны попадать в журнал в момент прихода, пока снимок ещё идёт"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn closed_channel_does_not_stop_the_snapshot() {
        let journal = Journal { snapshot_starts: AtomicUsize::new(0) };
        let (tx, rx) = mpsc::channel(8);
        tx.send("only".to_string()).await.unwrap();
        drop(tx);
        let mut log = Vec::new();
        let start = Instant::now();
        let result = snapshot_and_log(&journal, rx, &mut log).await;
        assert_eq!(result, 42, "канал закрыт, а снимок надо доделать");
        assert_eq!(start.elapsed(), ms(1000), "закрытие канала не должно ни обрывать снимок, ни ускорять его");
        assert_eq!(log.len(), 1, "единственное сообщение записано");
    }
}
