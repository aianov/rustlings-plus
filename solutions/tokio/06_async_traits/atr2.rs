use std::sync::Arc;

trait UserRepo {
    async fn find(&self, id: u32) -> Option<String>;
}

async fn warm<R: UserRepo + Send + Sync + 'static>(repo: Arc<R>, ids: Vec<u32>) -> usize {
    let lookups = ids.iter().map(|id| repo.find(*id));
    futures::future::join_all(lookups).await.into_iter().flatten().count()
}

struct DemoRepo;

impl UserRepo for DemoRepo {
    async fn find(&self, id: u32) -> Option<String> {
        (id < 100).then(|| format!("user{id}"))
    }
}

#[tokio::main]
async fn main() {
    println!("{}", warm(Arc::new(DemoRepo), vec![1, 2, 500]).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::time::Duration;
    use tokio::time::{Instant, sleep};

    struct SlowRepo {
        known: HashSet<u32>,
    }

    impl UserRepo for SlowRepo {
        async fn find(&self, id: u32) -> Option<String> {
            sleep(Duration::from_millis(50)).await;
            self.known.contains(&id).then(|| format!("user{id}"))
        }
    }

    fn repo() -> Arc<SlowRepo> {
        Arc::new(SlowRepo { known: [1, 2, 3].into_iter().collect() })
    }

    #[tokio::test(start_paused = true)]
    async fn counts_only_existing_users() {
        let found = warm(repo(), vec![1, 2, 3, 99, 2]).await;
        assert_eq!(found, 4, "в списке четыре известных id (с повтором) и один неизвестный");
        assert_eq!(warm(repo(), vec![]).await, 0, "пустой список: никого не нашли");
    }

    #[tokio::test(start_paused = true)]
    async fn lookups_run_together() {
        let start = Instant::now();
        warm(repo(), (0..40).collect()).await;
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(50),
            "сорок обращений по 50 мс должны идти вместе, а не по очереди"
        );
    }
}
