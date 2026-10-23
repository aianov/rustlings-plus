
use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::sync::Arc;
use tokio::sync::OnceCell;

fn main() {}

#[derive(Default)]
pub struct SingleFlight {
    inflight: Mutex<HashMap<String, Arc<OnceCell<String>>>>,
}

impl SingleFlight {
    pub fn new() -> SingleFlight {
        SingleFlight::default()
    }

    /// Сколько ключей сейчас в полёте.
    pub fn inflight(&self) -> usize {
        self.inflight.lock().unwrap().len()
    }

    pub async fn run<Fut: Future<Output = String>>(&self, key: &str, load: impl FnOnce() -> Fut) -> String {
        let cell = self.inflight.lock().unwrap().entry(key.to_string()).or_default().clone();
        let value = cell.get_or_init(load).await.clone();
        let mut map = self.inflight.lock().unwrap();
        if map.get(key).is_some_and(|current| Arc::ptr_eq(current, &cell)) {
            map.remove(key);
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use tokio::time::Instant;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    async fn slow_db(calls: &AtomicU32, key: &str) -> String {
        calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(MS(100)).await;
        format!("строка {key}")
    }

    #[tokio::test(start_paused = true)]
    async fn a_crowd_for_one_key_makes_one_call() {
        let sf = SingleFlight::new();
        let calls = AtomicU32::new(0);
        let start = Instant::now();
        let all = join_all((0..30).map(|_| sf.run("star", || slow_db(&calls, "star")))).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "тридцать одновременных запросов вызвали базу {} раз", calls.load(Ordering::SeqCst));
        assert!(all.iter().all(|v| v == "строка star"), "все получают один результат");
        assert_eq!(start.elapsed(), MS(100), "ждать нужно один запрос, а не очередь из тридцати");
    }

    #[tokio::test(start_paused = true)]
    async fn different_keys_do_not_wait_for_each_other() {
        let sf = SingleFlight::new();
        let calls = AtomicU32::new(0);
        let start = Instant::now();
        let all = join_all((0..5).map(|i| {
            let key = format!("k{i}");
            let sf = &sf;
            let calls = &calls;
            async move { sf.run(&key, || slow_db(calls, &key)).await }
        }))
        .await;
        assert_eq!(all.len(), 5);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
        assert_eq!(start.elapsed(), MS(100), "пять разных ключей должны идти параллельно");
    }

    #[tokio::test(start_paused = true)]
    async fn after_completion_the_next_call_loads_again() {
        let sf = SingleFlight::new();
        let calls = AtomicU32::new(0);
        sf.run("k", || slow_db(&calls, "k")).await;
        assert_eq!(sf.inflight(), 0, "завершённый ключ не должен оставаться в карте");
        sf.run("k", || slow_db(&calls, "k")).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "single-flight не кэш: после завершения данные берутся заново");
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_leader_does_not_strand_the_waiters() {
        let sf = Arc::new(SingleFlight::new());
        let calls = Arc::new(AtomicU32::new(0));
        let (s, c) = (sf.clone(), calls.clone());
        let leader = tokio::spawn(async move { s.run("k", || slow_db(&c, "k")).await });
        tokio::time::sleep(MS(10)).await;
        let (s, c) = (sf.clone(), calls.clone());
        let waiter = tokio::spawn(async move { s.run("k", || slow_db(&c, "k")).await });
        tokio::time::sleep(MS(10)).await;
        leader.abort();
        let got = tokio::time::timeout(Duration::from_secs(5), waiter).await.expect("ждущий завис после отмены выполняющего").unwrap();
        assert_eq!(got, "строка k");
        assert_eq!(sf.inflight(), 0);
    }
}
