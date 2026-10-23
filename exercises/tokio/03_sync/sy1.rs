// Счётчик посещений страниц, общий для сотни задач. Состояние простое (карта со счётчиками),
// а работа с ним короткая. Обычный std::sync::Mutex отлично подходит, если внутри замка нет
// ни одного .await: пока задача держит замок, она не отдаёт поток, и остальные ждут очень недолго.
// Медленная часть (`audit`, она async) не должна происходить под замком.
// TODO: Допиши `record` (сначала audit, потом короткое обновление под замком) и `average`.
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
        todo!()
    }

    fn average(&self, route: &str) -> Option<f64> {
        todo!()
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
