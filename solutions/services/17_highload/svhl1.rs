
use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

pub struct Cache {
    ttl: Duration,
    entries: Mutex<HashMap<String, (String, Instant)>>,
}

impl Cache {
    pub fn new(ttl: Duration) -> Cache {
        Cache { ttl, entries: Mutex::new(HashMap::new()) }
    }

    /// Живое значение или `None`. Запись, срок которой вышел, не отдаётся.
    pub fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        entries.get(key).filter(|(_, at)| at.elapsed() < self.ttl).map(|(v, _)| v.clone())
    }

    pub fn put(&self, key: &str, value: &str) {
        self.entries.lock().unwrap().insert(key.to_string(), (value.to_string(), Instant::now()));
    }

    pub fn invalidate(&self, key: &str) {
        self.entries.lock().unwrap().remove(key);
    }

    /// Значение из кэша или результат `load`, который запоминается. Ошибка `load` уходит наружу и не запоминается.
    pub async fn get_or_load<E, Fut: Future<Output = Result<String, E>>>(&self, key: &str, load: impl FnOnce() -> Fut) -> Result<String, E> {
        if let Some(hit) = self.get(key) {
            return Ok(hit);
        }
        let value = load().await?;
        self.put(key, &value);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const S: fn(u64) -> Duration = Duration::from_secs;

    async fn loaded(calls: &AtomicU32, value: &str) -> Result<String, &'static str> {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(value.to_string())
    }

    #[tokio::test(start_paused = true)]
    async fn the_second_read_does_not_touch_the_source() {
        let cache = Cache::new(S(60));
        let calls = AtomicU32::new(0);
        assert_eq!(cache.get_or_load("u1", || loaded(&calls, "Аня")).await, Ok("Аня".to_string()));
        assert_eq!(cache.get_or_load("u1", || loaded(&calls, "другое")).await, Ok("Аня".to_string()), "второй раз ответ берётся из кэша");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "источник вызван {} раз вместо одного", calls.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn entries_expire() {
        let cache = Cache::new(S(60));
        cache.put("u1", "старое");
        tokio::time::advance(S(59)).await;
        assert_eq!(cache.get("u1").as_deref(), Some("старое"));
        tokio::time::advance(S(2)).await;
        assert_eq!(cache.get("u1"), None, "срок жизни 60 секунд прошёл");
        let calls = AtomicU32::new(0);
        assert_eq!(cache.get_or_load("u1", || loaded(&calls, "новое")).await.unwrap(), "новое");
    }

    #[tokio::test(start_paused = true)]
    async fn keys_are_independent_and_invalidation_works() {
        let cache = Cache::new(S(60));
        cache.put("a", "1");
        cache.put("b", "2");
        cache.invalidate("a");
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b").as_deref(), Some("2"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_load_is_not_remembered() {
        let cache = Cache::new(S(60));
        let r: Result<String, &str> = cache.get_or_load("u1", || async { Err("база недоступна") }).await;
        assert_eq!(r, Err("база недоступна"));
        assert_eq!(cache.get("u1"), None, "ошибка не должна попасть в кэш как значение");
        let calls = AtomicU32::new(0);
        assert_eq!(cache.get_or_load("u1", || loaded(&calls, "ожила")).await.unwrap(), "ожила", "следующий вызов пробует источник снова");
    }
}
