// Кэш с временем жизни записей. Срок жизни час, и тестировать его живым временем было бы
// мучением, поэтому тесты идут на замороженных часах: рантайм сам перематывает время, когда
// все задачи ждут. Часы кэша должны быть теми же, что видит рантайм.
// Требования: запись живёт ровно ttl и в момент ttl уже считается просроченной; `get` отдаёт
// значение или None; `remaining` показывает, сколько записи осталось жить; `purge_expired`
// выбрасывает просроченное и возвращает, сколько выбросил.
// TODO: Допиши все четыре метода.
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;

struct TtlCache<V> {
    ttl: Duration,
    entries: HashMap<String, (V, Instant)>,
}

impl<V: Clone> TtlCache<V> {
    fn new(ttl: Duration) -> Self {
        TtlCache { ttl, entries: HashMap::new() }
    }

    fn insert(&mut self, key: &str, value: V) {
        todo!()
    }

    fn get(&self, key: &str) -> Option<V> {
        todo!()
    }

    fn remaining(&self, key: &str) -> Option<Duration> {
        todo!()
    }

    fn purge_expired(&mut self) -> usize {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let mut cache = TtlCache::new(Duration::from_secs(60));
    cache.insert("user", 1);
    println!("{:?} {:?} {}", cache.get("user"), cache.remaining("user"), cache.purge_expired());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    const HOUR: Duration = Duration::from_secs(3600);

    #[tokio::test(start_paused = true)]
    async fn entry_lives_exactly_ttl() {
        let mut cache = TtlCache::new(HOUR);
        cache.insert("a", 1);
        sleep(Duration::from_secs(3599)).await;
        assert_eq!(cache.get("a"), Some(1), "за секунду до срока запись жива");
        sleep(Duration::from_secs(1)).await;
        assert_eq!(cache.get("a"), None, "ровно через ttl запись уже просрочена");
    }

    #[tokio::test(start_paused = true)]
    async fn remaining_counts_down() {
        let mut cache = TtlCache::new(HOUR);
        cache.insert("a", 1);
        sleep(Duration::from_secs(600)).await;
        assert_eq!(cache.remaining("a"), Some(Duration::from_secs(3000)), "осталось 3000 секунд");
        assert_eq!(cache.remaining("missing"), None, "для чужого ключа None");
        sleep(HOUR).await;
        assert_eq!(cache.remaining("a"), None, "у просроченной записи ничего не осталось");
    }

    #[tokio::test(start_paused = true)]
    async fn purge_drops_only_expired() {
        let mut cache = TtlCache::new(HOUR);
        cache.insert("old", 1);
        sleep(Duration::from_secs(1800)).await;
        cache.insert("new", 2);
        sleep(Duration::from_secs(1800)).await;
        assert_eq!(cache.purge_expired(), 1, "просрочена одна запись, old");
        assert_eq!(cache.get("new"), Some(2), "новая запись осталась");
        assert_eq!(cache.purge_expired(), 0, "повторная уборка ничего не находит");
    }

    #[tokio::test(start_paused = true)]
    async fn insert_again_renews_the_term() {
        let mut cache = TtlCache::new(HOUR);
        cache.insert("a", 1);
        sleep(Duration::from_secs(3000)).await;
        cache.insert("a", 2);
        sleep(Duration::from_secs(3000)).await;
        assert_eq!(cache.get("a"), Some(2), "повторная вставка продлевает срок и меняет значение");
    }
}
