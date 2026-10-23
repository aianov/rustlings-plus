// Кто-то перебирает идентификаторы профилей: /user/1, /user/2, /user/3... Почти всех этих пользователей нет, кэш на них не срабатывает
// (в кэш кладут только найденное), и каждый запрос идёт в базу. Нагрузка на базу такая же, как без кэша. Помнить нужно и то, что
// «ничего не найдено», но недолго: пользователь может зарегистрироваться через минуту.
// Цель: найденный профиль живёт долго (`found_ttl`), отсутствие профиля коротко (`missing_ttl`), записей об отсутствии не больше
// `missing_cap`: перебор миллиона идентификаторов не должен выедать память и выталкивать настоящие профили.


use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

enum Entry {
    Found(String),
    Missing,
}

pub struct ProfileCache {
    found_ttl: Duration,
    missing_ttl: Duration,
    missing_cap: usize,
    entries: Mutex<HashMap<u64, (Entry, Instant)>>,
}

impl ProfileCache {
    pub fn new(found_ttl: Duration, missing_ttl: Duration, missing_cap: usize) -> ProfileCache {
        ProfileCache { found_ttl, missing_ttl, missing_cap, entries: Mutex::new(HashMap::new()) }
    }

    /// Сколько записей об отсутствии хранится.
    pub fn missing_len(&self) -> usize {
        self.entries.lock().unwrap().values().filter(|(e, _)| matches!(e, Entry::Missing)).count()
    }

    pub fn invalidate(&self, id: u64) {
        self.entries.lock().unwrap().remove(&id);
    }

    pub async fn get<Fut: Future<Output = Option<String>>>(&self, id: u64, load: impl FnOnce() -> Fut) -> Option<String> {
        let _ = (self.found_ttl, self.missing_ttl, self.missing_cap, &self.entries, Entry::Missing, Instant::now());
        load().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const S: fn(u64) -> Duration = Duration::from_secs;

    async fn lookup(calls: &AtomicU32, exists: bool) -> Option<String> {
        calls.fetch_add(1, Ordering::SeqCst);
        exists.then(|| "профиль".to_string())
    }

    #[tokio::test(start_paused = true)]
    async fn a_missing_user_is_remembered_for_a_short_time() {
        let cache = ProfileCache::new(S(600), S(5), 1000);
        let calls = AtomicU32::new(0);
        for _ in 0..1000 {
            assert_eq!(cache.get(404, || lookup(&calls, false)).await, None);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "1000 запросов про одного несуществующего: в базу ушёл один");
        tokio::time::advance(S(6)).await;
        cache.get(404, || lookup(&calls, false)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "через 6 секунд память об отсутствии устарела, нужна новая проверка");
    }

    #[tokio::test(start_paused = true)]
    async fn a_user_who_registers_appears_after_the_short_ttl() {
        let cache = ProfileCache::new(S(600), S(5), 1000);
        let calls = AtomicU32::new(0);
        assert_eq!(cache.get(7, || lookup(&calls, false)).await, None);
        tokio::time::advance(S(6)).await;
        assert_eq!(cache.get(7, || lookup(&calls, true)).await.as_deref(), Some("профиль"));
        tokio::time::advance(S(100)).await;
        cache.get(7, || lookup(&calls, true)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "найденное живёт долго");
    }

    #[tokio::test(start_paused = true)]
    async fn explicit_invalidation_forgets_a_missing_entry() {
        let cache = ProfileCache::new(S(600), S(5), 1000);
        let calls = AtomicU32::new(0);
        cache.get(7, || lookup(&calls, false)).await;
        cache.invalidate(7);
        assert!(cache.get(7, || lookup(&calls, true)).await.is_some(), "после события регистрации ждать не нужно");
    }

    #[tokio::test(start_paused = true)]
    async fn enumeration_cannot_eat_memory_or_push_out_real_profiles() {
        let cache = ProfileCache::new(S(600), S(5), 1000);
        let calls = AtomicU32::new(0);
        cache.get(1, || lookup(&calls, true)).await;
        for id in 10_000..15_000 {
            cache.get(id, || lookup(&calls, false)).await;
        }
        assert!(cache.missing_len() <= 1000, "записей об отсутствии {}: предел 1000", cache.missing_len());
        let before = calls.load(Ordering::SeqCst);
        assert!(cache.get(1, || lookup(&calls, true)).await.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), before, "настоящий профиль не должен был вылететь из-за перебора");
    }
}
