// Склад пополняется методом `restock`: добавить товар и вернуть общее количество на складе. Один
// вызов работает, но в тестах метод зависает намертво, хотя других задач с замком нет и ничего
// не блокирует его снаружи. А ещё, если замок отпустить посреди операции, общее количество в
// ответах параллельных пополнений перестаёт быть точным.
// Разберись, кто кого ждёт, и почини `restock`.
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

struct Inventory {
    items: Mutex<HashMap<String, u32>>,
}

impl Inventory {
    fn new() -> Inventory {
        Inventory { items: Mutex::new(HashMap::new()) }
    }

    async fn add(&self, name: &str, count: u32) {
        let mut items = self.items.lock().await;
        sleep(Duration::from_millis(1)).await;
        *items.entry(name.to_string()).or_default() += count;
    }

    async fn total(&self) -> u32 {
        self.items.lock().await.values().sum()
    }

    async fn restock(&self, name: &str, count: u32) -> u32 {
        let guard = self.items.lock().await;
        let known = guard.len();
        self.add(name, count).await;
        let total = self.total().await;
        drop(guard);
        let _ = known;
        total
    }
}

#[tokio::main]
async fn main() {
    let inventory = Inventory::new();
    println!("{}", inventory.restock("яблоки", 3).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::timeout;

    #[tokio::test(start_paused = true)]
    async fn restock_returns_the_total_without_hanging() {
        let inventory = Inventory::new();
        inventory.add("яблоки", 5).await;
        let total = timeout(Duration::from_secs(1), inventory.restock("груши", 3)).await.expect("restock завис: вызывает методы, которые берут тот же замок");
        assert_eq!(total, 8, "5 яблок и 3 груши");
        assert_eq!(inventory.total().await, 8);
    }

    #[tokio::test(start_paused = true)]
    async fn parallel_restocks_each_see_their_own_total() {
        let inventory = Arc::new(Inventory::new());
        let jobs: Vec<_> = (0..3)
            .map(|_| {
                let inventory = inventory.clone();
                tokio::spawn(async move { inventory.restock("яблоки", 10).await })
            })
            .collect();
        let mut seen = Vec::new();
        for job in jobs {
            seen.push(timeout(Duration::from_secs(1), job).await.expect("пополнение зависло").unwrap());
        }
        seen.sort();
        assert_eq!(seen, vec![10, 20, 30], "каждое пополнение видит итог сразу после своей добавки: замок нельзя отпускать посреди операции");
    }
}
