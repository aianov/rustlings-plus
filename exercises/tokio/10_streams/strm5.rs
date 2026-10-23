// Загрузка страниц профилей: на каждую уходит 100 мс. Для десяти страниц это целая секунда,
// и так долго ждать никто не хочет. Бэкенд при этом выдерживает не больше четырёх
// одновременных запросов, а на выходе страницы должны идти в порядке номеров. Сейчас функция
// всё делает правильно по результату, но слишком медленно. Ускорь её, не нарушив лимит и порядок.
use futures::stream::{self, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

struct Backend {
    skew: bool,
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Backend {
    fn new(skew: bool) -> Backend {
        Backend { skew, active: AtomicUsize::new(0), peak: AtomicUsize::new(0) }
    }

    async fn fetch(&self, id: u32) -> String {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        let ms = if self.skew { 50 * (10 - u64::from(id)).max(1) } else { 100 };
        sleep(Duration::from_millis(ms)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        format!("page{id}")
    }
}

async fn fetch_all(backend: &Backend, ids: Vec<u32>, limit: usize) -> Vec<String> {
    let _ = limit;
    stream::iter(ids).then(|id| backend.fetch(id)).collect().await
}

#[tokio::main]
async fn main() {
    let backend = Backend::new(false);
    println!("{:?}", fetch_all(&backend, vec![1, 2, 3], 2).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn four_at_a_time() {
        let backend = Backend::new(false);
        let started = Instant::now();
        let pages = fetch_all(&backend, (1..=10).collect(), 4).await;
        assert_eq!(pages.len(), 10, "должны вернуться все десять страниц");
        assert_eq!(backend.peak.load(Ordering::SeqCst), 4, "одновременно должно идти ровно четыре запроса, не больше лимита и не меньше");
        assert_eq!(started.elapsed(), Duration::from_millis(300), "десять запросов по 100 мс при четырёх в работе: три «волны», 300 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn order_survives_uneven_delays() {
        let backend = Backend::new(true);
        let pages = fetch_all(&backend, (1..=6).collect(), 3).await;
        let expected: Vec<String> = (1..=6).map(|i| format!("page{i}")).collect();
        assert_eq!(pages, expected, "ранние страницы медленнее поздних, но выдавать их надо по порядку номеров");
    }
}
