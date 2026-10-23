use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq)]
struct Profile {
    id: u32,
    name: String,
}

struct Db;

impl Db {
    async fn profile(&self, id: u32) -> Profile {
        let delay = 20 * (10 - u64::from(id % 10));
        sleep(Duration::from_millis(delay)).await;
        Profile { id, name: format!("user{id}") }
    }
}

async fn profiles(db: &Db, ids: &[u32]) -> Vec<Profile> {
    futures::future::join_all(ids.iter().map(|id| db.profile(*id))).await
}

#[tokio::main]
async fn main() {
    let found = profiles(&Db, &[1, 2, 3]).await;
    println!("{found:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn keeps_the_order_of_ids() {
        let found = profiles(&Db, &[1, 2, 3, 4, 5]).await;
        let ids: Vec<u32> = found.iter().map(|profile| profile.id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5], "профили должны идти в порядке списка, а не в порядке готовности (у id 1 самый медленный ответ)");
        assert_eq!(found[2].name, "user3", "имя берётся из ответа хранилища");
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_slowest_only() {
        let start = Instant::now();
        profiles(&Db, &[1, 2, 3, 4, 5]).await;
        assert_eq!(start.elapsed(), Duration::from_millis(180), "ответ id 1 идёт 180 мс, остальные успевают за это время");
    }

    #[tokio::test(start_paused = true)]
    async fn any_length_is_fine() {
        assert!(profiles(&Db, &[]).await.is_empty(), "пустой список даёт пустой ответ");
        let ids = [3u32; 100];
        let start = Instant::now();
        let found = profiles(&Db, &ids).await;
        assert_eq!(found.len(), 100, "повторы не схлопываются");
        assert_eq!(start.elapsed(), Duration::from_millis(140), "сто запросов тоже идут вместе, а не пачками");
    }
}
