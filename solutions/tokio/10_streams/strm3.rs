use futures::stream::{self, StreamExt};
use std::future::ready;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default)]
struct Directory {
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Directory {
    async fn lookup(&self, id: u32) -> Option<String> {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        sleep(Duration::from_millis(10)).await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        (id % 3 != 0).then(|| format!("user{id}"))
    }
}

async fn names(directory: &Directory, ids: Vec<u32>) -> Vec<String> {
    stream::iter(ids)
        .then(|id| directory.lookup(id))
        .filter_map(ready)
        .collect()
        .await
}

#[tokio::main]
async fn main() {
    let directory = Directory::default();
    println!("{:?}", names(&directory, vec![1, 2, 3]).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn found_names_in_order_one_at_a_time() {
        let directory = Directory::default();
        let started = Instant::now();
        let got = names(&directory, (1..=9).collect()).await;
        assert_eq!(got, vec!["user1", "user2", "user4", "user5", "user7", "user8"], "ответ: найденные имена в порядке номеров");
        assert_eq!(directory.peak.load(Ordering::SeqCst), 1, "справочник банит за параллельные запросы: одновременно не больше одного");
        assert_eq!(started.elapsed(), Duration::from_millis(90), "девять запросов по 10 мс подряд занимают 90 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn empty_list() {
        let directory = Directory::default();
        assert!(names(&directory, vec![]).await.is_empty(), "пустой список номеров: пустой ответ");
    }
}
