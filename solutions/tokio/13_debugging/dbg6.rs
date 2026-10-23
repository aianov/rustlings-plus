use tokio::sync::RwLock;

struct Registry {
    names: RwLock<Vec<String>>,
}

impl Registry {
    fn new() -> Registry {
        Registry { names: RwLock::new(Vec::new()) }
    }

    async fn add(&self, name: &str) {
        self.names.write().await.push(name.to_string());
    }

    async fn count(&self) -> usize {
        self.names.read().await.len()
    }

    async fn report(&self) -> String {
        let names = self.names.read().await;
        tokio::task::yield_now().await;
        format!("{}: {}", names.len(), names.join(","))
    }
}

#[tokio::main]
async fn main() {
    let registry = Registry::new();
    registry.add("анна").await;
    println!("{}", registry.report().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn report_works_without_writers() {
        let registry = Registry::new();
        registry.add("анна").await;
        registry.add("борис").await;
        assert_eq!(registry.report().await, "2: анна,борис", "без писателей отчёт читается как обычно");
        assert_eq!(registry.count().await, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn report_survives_a_writer_in_the_queue() {
        let registry = Arc::new(Registry::new());
        registry.add("анна").await;
        registry.add("борис").await;
        let writer = {
            let registry = registry.clone();
            tokio::spawn(async move { registry.add("вера").await })
        };
        let text = timeout(Duration::from_secs(1), registry.report())
            .await
            .expect("отчёт завис: читатель ждёт второго чтения, а писатель в очереди ждёт, пока читатель отпустит первое");
        writer.await.unwrap();
        let (count, list) = text.split_once(": ").unwrap();
        assert_eq!(count.parse::<usize>().unwrap(), list.split(',').count(), "число и список в отчёте должны быть из одного момента: {text}");
        assert_eq!(registry.count().await, 3, "писатель добавил третье имя");
    }
}
