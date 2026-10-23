use course_support::redis::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Profile {
    name: String,
    bio: String,
}

async fn get_profile(db: &PgPool, c: &Client, ns: &str, id: i64) -> Result<Option<Profile>, BoxError> {
    let key = format!("{ns}:profile:{id}");
    if let Some(json) = c.cmd(&["GET", &key]).await?.text() {
        return Ok(Some(serde_json::from_str(&json)?));
    }
    let row = sqlx::query_as::<_, (String, String)>("SELECT name, bio FROM profiles WHERE id = $1").bind(id).fetch_optional(db).await?;
    let Some((name, bio)) = row else {
        return Ok(None);
    };
    let profile = Profile { name, bio };
    c.cmd(&["SET", &key, &serde_json::to_string(&profile)?, "EX", "300"]).await?;
    Ok(Some(profile))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE profiles (id bigint PRIMARY KEY, name text NOT NULL, bio text NOT NULL); INSERT INTO profiles VALUES (7, 'kate', 'пишу на Rust'), (9, 'bob', 'просто bob')";

    #[tokio::test]
    async fn second_read_skips_the_database() {
        let db = TestDb::with_sql(SEED).await;
        let (pool, queries) = db.counted_pool(2).await;
        let s = Scope::new().await;
        let c = s.client();
        let want = Profile { name: "kate".to_string(), bio: "пишу на Rust".to_string() };
        assert_eq!(get_profile(&pool, &c, s.prefix(), 7).await.expect("первое чтение"), Some(want.clone()));
        let after_first = queries.get();
        assert!(after_first >= 1, "первое чтение идёт в базу");
        assert_eq!(get_profile(&pool, &c, s.prefix(), 7).await.expect("второе чтение"), Some(want));
        assert_eq!(queries.get(), after_first, "второе чтение берёт профиль из кэша, база не нужна");
        let ttl = s.cmd(&["TTL", &s.key("profile:7")]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=300).contains(&ttl), "кэш на 5 минут, TTL = {ttl}");
        assert_eq!(get_profile(&pool, &c, s.prefix(), 404).await.expect("нет профиля"), None);
    }
}
