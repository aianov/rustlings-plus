use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

trait Loader: Send + Sync + 'static {
    fn load(&self, key: String) -> BoxFuture<'static, String>;
}

trait Lookup: Send + Sync + 'static {
    fn build(loader: Arc<dyn Loader>) -> Self;
    fn get(&self, key: &str) -> BoxFuture<'_, String>;
}

type Inflight = Mutex<HashMap<String, Shared<BoxFuture<'static, String>>>>;

struct Good {
    loader: Arc<dyn Loader>,
    inflight: Inflight,
}

impl Lookup for Good {
    fn build(loader: Arc<dyn Loader>) -> Good {
        Good { loader, inflight: Mutex::new(HashMap::new()) }
    }

    fn get(&self, key: &str) -> BoxFuture<'_, String> {
        let key = key.to_string();
        async move {
            let shared = self.inflight.lock().unwrap().entry(key.clone()).or_insert_with(|| self.loader.load(key.clone()).shared()).clone();
            let value = shared.await;
            self.inflight.lock().unwrap().remove(&key);
            value
        }
        .boxed()
    }
}

struct NoDedup {
    loader: Arc<dyn Loader>,
}

impl Lookup for NoDedup {
    fn build(loader: Arc<dyn Loader>) -> NoDedup {
        NoDedup { loader }
    }

    fn get(&self, key: &str) -> BoxFuture<'_, String> {
        self.loader.load(key.to_string())
    }
}

struct NeverForget {
    loader: Arc<dyn Loader>,
    inflight: Inflight,
}

impl Lookup for NeverForget {
    fn build(loader: Arc<dyn Loader>) -> NeverForget {
        NeverForget { loader, inflight: Mutex::new(HashMap::new()) }
    }

    fn get(&self, key: &str) -> BoxFuture<'_, String> {
        let key = key.to_string();
        async move {
            let shared = self.inflight.lock().unwrap().entry(key.clone()).or_insert_with(|| self.loader.load(key.clone()).shared()).clone();
            shared.await
        }
        .boxed()
    }
}

struct OneSlot {
    loader: Arc<dyn Loader>,
    slot: Mutex<Option<Shared<BoxFuture<'static, String>>>>,
}

impl Lookup for OneSlot {
    fn build(loader: Arc<dyn Loader>) -> OneSlot {
        OneSlot { loader, slot: Mutex::new(None) }
    }

    fn get(&self, key: &str) -> BoxFuture<'_, String> {
        let key = key.to_string();
        async move {
            let shared = self.slot.lock().unwrap().get_or_insert_with(|| self.loader.load(key).shared()).clone();
            let value = shared.await;
            *self.slot.lock().unwrap() = None;
            value
        }
        .boxed()
    }
}

async fn settle() {
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
}

#[derive(Default)]
struct Inner {
    calls: usize,
    waiting: HashMap<String, Vec<oneshot::Sender<String>>>,
}

struct Gated {
    inner: Mutex<Inner>,
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Passed,
    Failed,
    Hung,
}

fn verdict<F: FnOnce() + Send + 'static>(secs: u64, run: F) -> Verdict {
    let (done, finished) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .spawn(move || {
            run();
            let _ = done.send(());
        })
        .unwrap();
    match finished.recv_timeout(Duration::from_secs(secs)) {
        Ok(()) => {
            worker.join().unwrap();
            Verdict::Passed
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Verdict::Failed,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Verdict::Hung,
    }
}

impl Gated {
    fn new() -> Gated {
        Gated { inner: Mutex::new(Inner::default()) }
    }

    fn calls(&self) -> usize {
        self.inner.lock().unwrap().calls
    }

    fn waiting(&self, key: &str) -> usize {
        self.inner.lock().unwrap().waiting.get(key).map_or(0, Vec::len)
    }

    fn release(&self, key: &str, value: &str) -> usize {
        let senders = self.inner.lock().unwrap().waiting.remove(key).unwrap_or_default();
        let count = senders.len();
        for sender in senders {
            let _ = sender.send(value.to_string());
        }
        count
    }
}

impl Loader for Gated {
    fn load(&self, key: String) -> BoxFuture<'static, String> {
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.lock().unwrap();
            inner.calls += 1;
            inner.waiting.entry(key).or_default().push(tx);
        }
        async move { rx.await.expect("ворота выброшены, не открывшись") }.boxed()
    }
}

fn suite<C: Lookup>() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap();
    rt.block_on(async {
        let gated = Arc::new(Gated::new());
        let cache = Arc::new(C::build(gated.clone()));
        let (c1, c2) = (cache.clone(), cache.clone());
        let first = tokio::spawn(async move { c1.get("cat").await });
        let second = tokio::spawn(async move { c2.get("cat").await });
        settle().await;
        assert_eq!(gated.calls(), 1, "два одновременных запроса одного ключа должны вызвать загрузчик один раз");
        assert_eq!(gated.release("cat", "мяу"), 1, "ждущая загрузка по ключу ровно одна");
        assert_eq!(first.await.unwrap(), "мяу", "первый запрос получает значение");
        assert_eq!(second.await.unwrap(), "мяу", "второй получает то же значение, не ходя за ним снова");
        let c3 = cache.clone();
        let third = tokio::spawn(async move { c3.get("cat").await });
        settle().await;
        assert_eq!(gated.calls(), 2, "после завершения загрузки повторный запрос обязан снова идти к загрузчику");
        gated.release("cat", "мур");
        assert_eq!(third.await.unwrap(), "мур", "новая загрузка даёт новое значение");
        let (ca, cb) = (cache.clone(), cache.clone());
        let a = tokio::spawn(async move { ca.get("a").await });
        let b = tokio::spawn(async move { cb.get("b").await });
        settle().await;
        assert_eq!(gated.calls(), 4, "разные ключи загружаются независимо: по вызову на каждый");
        gated.release("a", "A");
        gated.release("b", "B");
        assert_eq!((a.await.unwrap(), b.await.unwrap()), ("A".to_string(), "B".to_string()), "значения не путаются между ключами");
    });
}

fn main() {
    suite::<Good>();
    println!("Good проходит");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_load_waits_until_its_gate_is_opened() {
        let gated = Gated::new();
        let pending = tokio::spawn(gated.load("a".to_string()));
        settle().await;
        assert!(!pending.is_finished(), "ворота закрыты: загрузка не должна завершаться");
        assert_eq!((gated.calls(), gated.waiting("a")), (1, 1), "один вызов, один ждущий");
        assert_eq!(gated.release("a", "x"), 1, "release возвращает число отпущенных загрузок");
        assert_eq!(pending.await.unwrap(), "x", "загрузка получает значение из release");
    }

    #[tokio::test(start_paused = true)]
    async fn gates_are_per_key_and_close_again_after_release() {
        let gated = Gated::new();
        let a = tokio::spawn(gated.load("a".to_string()));
        let b = tokio::spawn(gated.load("b".to_string()));
        settle().await;
        assert_eq!(gated.release("a", "1"), 1);
        assert_eq!(a.await.unwrap(), "1");
        settle().await;
        assert!(!b.is_finished(), "ворота ключа b остаются закрытыми, пока их не откроют");
        assert_eq!(gated.release("zzz", "?"), 0, "у неизвестного ключа никто не ждёт");
        let again = tokio::spawn(gated.load("a".to_string()));
        settle().await;
        assert!(!again.is_finished(), "отпущенные ворота не остаются открытыми навсегда: новая загрузка снова ждёт");
        gated.release("b", "2");
        gated.release("a", "3");
        assert_eq!((b.await.unwrap(), again.await.unwrap()), ("2".to_string(), "3".to_string()));
    }

    #[test]
    fn the_suite_passes_on_the_correct_cache() {
        assert_eq!(verdict(20, suite::<Good>), Verdict::Passed, "на исправном кеше тест обязан проходить");
    }

    #[test]
    fn the_suite_notices_a_cache_without_deduplication() {
        assert_eq!(verdict(20, suite::<NoDedup>), Verdict::Failed, "кеш, не склеивающий запросы, должен быть пойман");
    }

    #[test]
    fn the_suite_notices_a_cache_that_never_forgets() {
        assert_eq!(verdict(20, suite::<NeverForget>), Verdict::Failed, "кеш, навсегда запомнивший ответ, должен быть пойман");
    }

    #[test]
    fn the_suite_notices_mixed_up_keys() {
        assert_eq!(verdict(20, suite::<OneSlot>), Verdict::Failed, "кеш с одним слотом на все ключи должен быть пойман");
    }
}
