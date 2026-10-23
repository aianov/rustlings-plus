// Счётчик просмотров поста лежит в KV под одним ключом, и его увеличивают сразу несколько
// экземпляров сервиса: читают значение, прибавляют единицу и записывают. Под нагрузкой счётчик
// показывает меньше настоящего: два экземпляра прочитали 41, оба записали 42, один просмотр
// пропал. Цель: increment прибавляет единицу так, чтобы одновременные вызовы с любых
// экземпляров ничего не теряли, и возвращает новое значение. Ключа может ещё не быть.


use async_nats::jetstream::kv::Store;

fn main() {}

pub async fn increment(kv: &Store, key: &str) -> Result<u64, String> {
    let current = kv.get(key).await.map_err(|e| e.to_string())?;
    let value = match current {
        Some(bytes) => String::from_utf8_lossy(&bytes).parse::<u64>().map_err(|e| e.to_string())? + 1,
        None => 1,
    };
    kv.put(key, value.to_string().into()).await.map_err(|e| e.to_string())?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::kv::Config;
    use course_support::nats::{Scope, connect, js};

    #[tokio::test]
    async fn concurrent_increments_from_two_instances_lose_nothing() {
        let scope = Scope::new();
        let first = js(&connect().await);
        let second = js(&connect().await);
        let bucket = scope.name("views");
        scope.track_bucket(&bucket);
        let kv_a = first.create_key_value(Config { bucket: bucket.clone(), history: 5, ..Default::default() }).await.unwrap();
        let kv_b = second.get_key_value(&bucket).await.unwrap();
        let mut tasks = Vec::new();
        for worker in 0..8 {
            let kv = if worker % 2 == 0 { kv_a.clone() } else { kv_b.clone() };
            tasks.push(tokio::spawn(async move {
                for _ in 0..15 {
                    increment(&kv, "post-7").await.expect("increment");
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        let total = kv_a.get("post-7").await.unwrap().map(|b| String::from_utf8_lossy(&b).into_owned());
        assert_eq!(total.as_deref(), Some("120"), "8 работников по 15 прибавлений дают ровно 120: каждое прибавление должно учитываться");
    }

    #[tokio::test]
    async fn the_returned_value_is_the_new_one_and_a_missing_key_starts_at_one() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let bucket = scope.name("views");
        scope.track_bucket(&bucket);
        let kv = js.create_key_value(Config { bucket, history: 5, ..Default::default() }).await.unwrap();
        assert_eq!(increment(&kv, "a").await, Ok(1), "ключа не было: первое прибавление даёт 1");
        assert_eq!(increment(&kv, "a").await, Ok(2), "второе даёт 2");
        assert_eq!(increment(&kv, "b").await, Ok(1), "другой ключ считается отдельно");
    }

    #[tokio::test]
    async fn a_deleted_counter_starts_over() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let bucket = scope.name("views");
        scope.track_bucket(&bucket);
        let kv = js.create_key_value(Config { bucket, history: 5, ..Default::default() }).await.unwrap();
        increment(&kv, "a").await.unwrap();
        increment(&kv, "a").await.unwrap();
        kv.delete("a").await.unwrap();
        assert_eq!(increment(&kv, "a").await, Ok(1), "после удаления счёт идёт с единицы");
    }
}
