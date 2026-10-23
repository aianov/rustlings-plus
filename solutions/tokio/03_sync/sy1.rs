use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Default)]
struct Stats {
    routes: Mutex<HashMap<String, (u32, u64)>>,
}

async fn audit(_route: &str) {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

impl Stats {
    async fn record(&self, route: &str, ms: u64) {
        audit(route).await;
        let mut routes = self.routes.lock().unwrap();
        let entry = routes.entry(route.to_string()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += ms;
    }

    fn average(&self, route: &str) -> Option<f64> {
        let routes = self.routes.lock().unwrap();
        let (count, total) = routes.get(route)?;
        Some(*total as f64 / f64::from(*count))
    }
}

#[tokio::main]
async fn main() {
    let stats = Stats::default();
    stats.record("/feed", 20).await;
    stats.record("/feed", 40).await;
    println!("{:?}", stats.average("/feed"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn average_of_two() {
        let stats = Stats::default();
        stats.record("/a", 10).await;
        stats.record("/a", 30).await;
        assert_eq!(stats.average("/a"), Some(20.0), "среднее по двум замерам");
        assert_eq!(stats.average("/b"), None, "по незнакомому маршруту среднего нет");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn hundred_tasks_do_not_lose_updates() {
        let stats = Arc::new(Stats::default());
        let handles: Vec<_> = (0..100)
            .map(|i| {
                let stats = Arc::clone(&stats);
                tokio::spawn(async move { stats.record("/hot", i % 10).await })
            })
            .collect();
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(stats.average("/hot"), Some(4.5), "100 замеров 0..9 по десять раз: среднее 4.5, потерь быть не должно");
    }
}
