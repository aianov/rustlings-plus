// Нужно обработать десять тысяч заданий, каждое с небольшой паузой (в проде это запрос в
// сервис). Сейчас они запускаются все сразу, и сервис падает под нагрузкой: больше 64
// одновременных запросов он не выдерживает. Порядок результатов не важен, нужна только сумма.
// Держи в работе не больше 64 и не растягивай обработку до бесконечности.
use futures::stream::{self, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

const LIMIT: usize = 64;

#[derive(Default)]
struct Service {
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Service {
    async fn work(&self, id: u32) -> u64 {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        sleep(Duration::from_millis(10)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        u64::from(id) * 2
    }
}

use futures::future::join_all;

async fn process_all(service: &Service, ids: Vec<u32>) -> u64 {
    let _ = (LIMIT, stream::empty::<u32>().map(|x| x));
    let results = join_all(ids.into_iter().map(|id| service.work(id))).await;
    results.into_iter().sum()
}

#[tokio::main]
async fn main() {
    let service = Service::default();
    println!("{}", process_all(&service, (0..100).collect()).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn ten_thousand_jobs_under_the_limit() {
        let service = Service::default();
        let started = Instant::now();
        let sum = process_all(&service, (0..10_000).collect()).await;
        let expected: u64 = (0..10_000u64).map(|i| i * 2).sum();
        assert_eq!(sum, expected, "сумма результатов всех заданий");
        let peak = service.peak.load(Ordering::SeqCst);
        assert!(peak <= LIMIT, "одновременно шло {peak} заданий, а сервис выдерживает {LIMIT}");
        assert!(peak >= LIMIT / 2, "одновременно шло только {peak}: обработка почти последовательная, а сервис способен на {LIMIT}");
        assert!(started.elapsed() < Duration::from_secs(2), "обработка заняла {:?}: при 64 в работе хватит около 1,6 секунды", started.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn small_batch() {
        let service = Service::default();
        assert_eq!(process_all(&service, vec![1, 2, 3]).await, 12, "три задания меньше лимита");
    }
}
