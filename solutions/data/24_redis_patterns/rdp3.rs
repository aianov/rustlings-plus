use course_support::redis::{Client, Error};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn ttl_for(id: i64) -> u64 {
    let mut h = DefaultHasher::new();
    id.hash(&mut h);
    300 + h.finish() % 60
}

async fn put(c: &Client, ns: &str, id: i64, json: &str) -> Result<(), Error> {
    c.cmd(&["SET", &format!("{ns}:profile:{id}"), json, "EX", &ttl_for(id).to_string()]).await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::collections::BTreeSet;

    #[tokio::test]
    async fn expiries_are_spread() {
        let s = Scope::new().await;
        let c = s.client();
        let mut ttls = BTreeSet::new();
        for id in 0..100 {
            put(&c, s.prefix(), id, "{}").await.expect("SET");
            let ttl = s.cmd(&["TTL", &s.key(&format!("profile:{id}"))]).await.expect("TTL").int().unwrap_or(-1);
            assert!((299..360).contains(&ttl), "срок от 300 до 359 секунд, а TTL = {ttl}");
            ttls.insert(ttl);
        }
        assert!(ttls.len() >= 20, "сроки должны быть разбросаны, а различных всего {}", ttls.len());
        assert_eq!(ttl_for(42), ttl_for(42), "один и тот же id получает один и тот же срок");
    }
}
