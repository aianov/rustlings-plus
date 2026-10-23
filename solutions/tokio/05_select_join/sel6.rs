use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;

struct Source {
    name: &'static str,
    delay_ms: u64,
    finished: AtomicBool,
}

impl Source {
    fn new(name: &'static str, delay_ms: u64) -> Source {
        Source { name, delay_ms, finished: AtomicBool::new(false) }
    }

    async fn fetch(&self, key: u32) -> String {
        sleep(Duration::from_millis(self.delay_ms)).await;
        self.finished.store(true, Ordering::SeqCst);
        format!("{}:{key}", self.name)
    }
}

async fn fastest(a: &Source, b: &Source, key: u32) -> String {
    tokio::select! {
        answer = a.fetch(key) => answer,
        answer = b.fetch(key) => answer,
    }
}

#[tokio::main]
async fn main() {
    let cache = Source::new("cache", 5);
    let db = Source::new("db", 50);
    println!("{}", fastest(&cache, &db, 1).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn second_source_can_win() {
        let (a, b) = (Source::new("cache", 300), Source::new("db", 100));
        let start = Instant::now();
        assert_eq!(fastest(&a, &b, 1).await, "db:1", "кэш медленнее базы, ответ должен прийти от базы");
        assert_eq!(start.elapsed(), Duration::from_millis(100), "ждём только победителя");
    }

    #[tokio::test(start_paused = true)]
    async fn first_source_can_win() {
        let (a, b) = (Source::new("cache", 20), Source::new("db", 100));
        assert_eq!(fastest(&a, &b, 2).await, "cache:2", "кэш быстрее, ответ должен прийти от кэша");
    }

    #[tokio::test(start_paused = true)]
    async fn loser_is_dropped_not_finished() {
        let (a, b) = (Source::new("cache", 300), Source::new("db", 100));
        fastest(&a, &b, 1).await;
        sleep(Duration::from_secs(5)).await;
        assert!(!a.finished.load(Ordering::SeqCst), "проигравший запрос дошёл до конца: его надо бросить, а не дожидаться");
        assert!(b.finished.load(Ordering::SeqCst), "победитель должен дойти до конца");
    }
}
