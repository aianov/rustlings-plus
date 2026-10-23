use async_nats::jetstream::kv::Store;

fn main() {}

pub async fn increment(kv: &Store, key: &str) -> Result<u64, String> {
    for _ in 0..200 {
        let current = kv.entry(key).await.map_err(|e| e.to_string())?;
        let (value, outcome) = match current {
            Some(entry) if entry.operation == async_nats::jetstream::kv::Operation::Put => {
                let value = String::from_utf8_lossy(&entry.value).parse::<u64>().map_err(|e| e.to_string())? + 1;
                (value, kv.update(key, value.to_string().into(), entry.revision).await.map(|_| ()).map_err(|e| e.to_string()))
            }
            _ => (1, kv.create(key, "1".to_string().into()).await.map(|_| ()).map_err(|e| e.to_string())),
        };
        if outcome.is_ok() {
            return Ok(value);
        }
    }
    Err("слишком много конфликтов подряд".to_string())
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
