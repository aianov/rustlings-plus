#![allow(dead_code)]
use std::collections::HashMap;
use tokio::time::Instant;
use std::time::Duration;

type Key = (String, u16);

struct Pool<S> {
    max_idle: usize,
    ttl: Duration,
    idle: HashMap<Key, Vec<(Instant, S)>>,
}

impl<S> Pool<S> {
    fn new(max_idle: usize, ttl: Duration) -> Pool<S> {
        Pool { max_idle, ttl, idle: HashMap::new() }
    }

    fn take(&mut self, key: &Key) -> Option<S> {
        let list = self.idle.get_mut(key)?;
        let (now, ttl) = (Instant::now(), self.ttl);
        list.retain(|(since, _)| now.duration_since(*since) < ttl);
        list.pop().map(|(_, s)| s)
    }

    fn put(&mut self, key: Key, conn: S) {
        let list = self.idle.entry(key).or_default();
        list.push((Instant::now(), conn));
        while list.len() > self.max_idle {
            list.remove(0);
        }
    }

    fn idle(&self) -> usize {
        self.idle.values().map(|list| list.len()).sum()
    }
}

fn main() {
    let mut pool: Pool<&str> = Pool::new(2, Duration::from_secs(30));
    pool.put(("example.com".to_string(), 80), "connection");
    println!("{:?}", pool.take(&("example.com".to_string(), 80)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Conn {
        id: u32,
        dropped: Arc<AtomicUsize>,
    }

    impl Drop for Conn {
        fn drop(&mut self) {
            self.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn key(host: &str, port: u16) -> Key {
        (host.to_string(), port)
    }

    fn pool(max_idle: usize, ttl_secs: u64) -> (Pool<Conn>, Arc<AtomicUsize>) {
        (Pool::new(max_idle, Duration::from_secs(ttl_secs)), Arc::new(AtomicUsize::new(0)))
    }

    fn conn(id: u32, dropped: &Arc<AtomicUsize>) -> Conn {
        Conn { id, dropped: dropped.clone() }
    }

    #[tokio::test(start_paused = true)]
    async fn put_then_take_gives_the_same_connection() {
        let (mut pool, dropped) = pool(4, 30);
        pool.put(key("a.test", 80), conn(1, &dropped));
        assert_eq!(pool.idle(), 1);
        let taken = pool.take(&key("a.test", 80)).expect("соединение лежит в пуле");
        assert_eq!(taken.id, 1);
        assert_eq!(pool.idle(), 0, "взятое соединение из пула уходит");
        assert!(pool.take(&key("a.test", 80)).is_none(), "второй раз взять то же соединение нельзя");
        assert_eq!(dropped.load(Ordering::SeqCst), 0, "соединение у нас в руках, закрывать его рано");
    }

    #[tokio::test(start_paused = true)]
    async fn keys_are_independent() {
        let (mut pool, dropped) = pool(4, 30);
        pool.put(key("a.test", 80), conn(1, &dropped));
        assert!(pool.take(&key("a.test", 8080)).is_none(), "другой порт это другой сервер");
        assert!(pool.take(&key("b.test", 80)).is_none(), "другой хост это другой сервер");
        assert!(pool.take(&key("a.test", 80)).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn freshest_connection_comes_first() {
        let (mut pool, dropped) = pool(4, 30);
        pool.put(key("a.test", 80), conn(1, &dropped));
        tokio::time::advance(Duration::from_secs(5)).await;
        pool.put(key("a.test", 80), conn(2, &dropped));
        assert_eq!(pool.take(&key("a.test", 80)).unwrap().id, 2, "самое свежее соединение с наименьшим шансом оказаться закрытым сервером");
        assert_eq!(pool.take(&key("a.test", 80)).unwrap().id, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn limit_is_per_host_and_drops_the_oldest() {
        let (mut pool, dropped) = pool(2, 30);
        for id in 1..=3 {
            pool.put(key("a.test", 80), conn(id, &dropped));
            tokio::time::advance(Duration::from_millis(10)).await;
        }
        assert_eq!(pool.idle(), 2, "больше двух простаивающих на хост не держим");
        assert_eq!(dropped.load(Ordering::SeqCst), 1, "лишнее соединение должно быть закрыто (Drop), а не потеряно в памяти");
        pool.put(key("b.test", 80), conn(10, &dropped));
        assert_eq!(pool.idle(), 3, "лимит считается по каждому хосту отдельно");
        assert_eq!(pool.take(&key("a.test", 80)).unwrap().id, 3);
        assert_eq!(pool.take(&key("a.test", 80)).unwrap().id, 2, "первое (самое старое) соединение вытеснено");
        assert!(pool.take(&key("a.test", 80)).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn stale_connections_are_not_handed_out() {
        let (mut pool, dropped) = pool(4, 30);
        pool.put(key("a.test", 80), conn(1, &dropped));
        tokio::time::advance(Duration::from_secs(29)).await;
        pool.put(key("a.test", 80), conn(2, &dropped));
        tokio::time::advance(Duration::from_secs(2)).await;
        let taken = pool.take(&key("a.test", 80)).expect("второе соединение простояло всего 2 с");
        assert_eq!(taken.id, 2);
        assert_eq!(dropped.load(Ordering::SeqCst), 1, "первое простояло 31 с и должно быть закрыто при взятии");
        assert!(pool.take(&key("a.test", 80)).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn everything_old_gives_none() {
        let (mut pool, dropped) = pool(4, 30);
        pool.put(key("a.test", 80), conn(1, &dropped));
        tokio::time::advance(Duration::from_secs(30)).await;
        assert!(pool.take(&key("a.test", 80)).is_none(), "ровно ttl простоя уже много: соединение протухло");
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert_eq!(pool.idle(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_resets_when_connection_is_returned() {
        let (mut pool, dropped) = pool(4, 30);
        pool.put(key("a.test", 80), conn(1, &dropped));
        tokio::time::advance(Duration::from_secs(25)).await;
        let c = pool.take(&key("a.test", 80)).unwrap();
        pool.put(key("a.test", 80), c);
        tokio::time::advance(Duration::from_secs(25)).await;
        assert!(pool.take(&key("a.test", 80)).is_some(), "простой считается от последнего возврата в пул, а не от создания соединения");
    }
}
