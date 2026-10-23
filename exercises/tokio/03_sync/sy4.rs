// Журнал присваивает записям порядковые номера и пишет их в медленное хранилище. Порядок записей
// в хранилище обязан совпадать с порядком номеров, даже если запись с большим номером пишется
// быстрее (хранилище так и делает: чем больше номер, тем меньше задержка). Поэтому присвоение
// номера и запись должны быть единым неделимым действием, а между ними есть .await. Для такого
// случая счётчик лежит под асинхронным замком tokio::sync::Mutex, чей guard можно держать через .await.
// Вторая функция запускает запись в фоне: замок берётся сразу, до возврата из функции, чтобы
// порядок вызовов совпал с порядком номеров, а сама запись идёт в spawn.
// TODO: Допиши `append` и `append_in_background`.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

#[derive(Default)]
struct Sink {
    written: Mutex<Vec<(u64, String)>>,
}

impl Sink {
    async fn write_slow(&self, seq: u64, line: &str) {
        let delay = 50u64.saturating_sub(seq * 10);
        tokio::time::sleep(Duration::from_millis(delay)).await;
        self.written.lock().unwrap().push((seq, line.to_string()));
    }
}

struct Log {
    next: Arc<AsyncMutex<u64>>,
    sink: Arc<Sink>,
}

impl Log {
    async fn append(&self, line: &str) -> u64 {
        todo!()
    }

    async fn append_in_background(&self, line: String) -> JoinHandle<u64> {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let log = Log { next: Arc::new(AsyncMutex::new(0)), sink: Arc::new(Sink::default()) };
    println!("{}", log.append("раз").await);
    let handle = log.append_in_background("два".to_string()).await;
    println!("{}", handle.await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_log() -> Arc<Log> {
        Arc::new(Log { next: Arc::new(AsyncMutex::new(0)), sink: Arc::new(Sink::default()) })
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_appends_stay_in_order() {
        let log = new_log();
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let log = Arc::clone(&log);
                tokio::spawn(async move { log.append(&format!("запись {i}")).await })
            })
            .collect();
        let mut seqs = Vec::new();
        for handle in handles {
            seqs.push(handle.await.unwrap());
        }
        seqs.sort();
        assert_eq!(seqs, [0, 1, 2, 3], "номера уникальны и идут подряд");
        let written = log.sink.written.lock().unwrap().clone();
        let order: Vec<u64> = written.iter().map(|(seq, _)| *seq).collect();
        assert_eq!(order, [0, 1, 2, 3], "в хранилище записи должны лечь в порядке номеров, а легли как {order:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn background_appends_follow_call_order() {
        let log = new_log();
        let a = log.append_in_background("a".to_string()).await;
        let b = log.append_in_background("b".to_string()).await;
        let c = log.append_in_background("c".to_string()).await;
        assert_eq!((a.await.unwrap(), b.await.unwrap(), c.await.unwrap()), (0, 1, 2), "номера по порядку вызовов");
        let written = log.sink.written.lock().unwrap().clone();
        assert_eq!(
            written.iter().map(|(_, line)| line.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"],
            "замок нужно взять до возврата из функции, тогда записи лягут в порядке вызовов"
        );
    }
}
