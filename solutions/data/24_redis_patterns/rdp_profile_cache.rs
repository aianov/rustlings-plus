use course_support::redis::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Profile {
    name: String,
    bio: String,
}

fn ttl_for(id: i64) -> u64 {
    let mut h = DefaultHasher::new();
    id.hash(&mut h);
    300 + h.finish() % 60
}

async fn load(db: &PgPool, id: i64) -> Result<Option<Profile>, BoxError> {
    let row = sqlx::query_as::<_, (String, String)>("SELECT name, bio FROM profiles WHERE id = $1").bind(id).fetch_optional(db).await?;
    Ok(row.map(|(name, bio)| Profile { name, bio }))
}

async fn get(db: &PgPool, c: &Client, ns: &str, id: i64) -> Result<Option<Profile>, BoxError> {
    let key = format!("{ns}:profile:{id}");
    let lock = format!("{key}:lock");
    for _ in 0..150 {
        if let Some(json) = c.cmd(&["GET", &key]).await?.text() {
            return Ok(serde_json::from_str(&json)?);
        }
        if !c.cmd(&["SET", &lock, "1", "NX", "PX", "2000"]).await?.is_nil() {
            let profile = load(db, id).await?;
            c.cmd(&["SET", &key, &serde_json::to_string(&profile)?, "EX", &ttl_for(id).to_string()]).await?;
            c.cmd(&["DEL", &lock]).await?;
            return Ok(profile);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("значение так и не появилось в кэше".into())
}

async fn rename(db: &PgPool, c: &Client, ns: &str, id: i64, name: &str) -> Result<(), BoxError> {
    let key = format!("{ns}:profile:{id}");
    sqlx::query("UPDATE profiles SET name = $1 WHERE id = $2").bind(name).bind(id).execute(db).await?;
    c.cmd(&["DEL", &key]).await?;
    for _ in 0..150 {
        if c.cmd(&["EXISTS", &format!("{key}:lock")]).await?.int() == Some(0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    c.cmd(&["DEL", &key]).await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE profiles (id bigint PRIMARY KEY, name text NOT NULL, bio text NOT NULL); INSERT INTO profiles SELECT g, 'user' || g, 'bio' FROM generate_series(1, 50) g";

    #[tokio::test]
    async fn cold_cache_is_filled_once() {
        let db = TestDb::with_sql(SEED).await;
        let (pool, queries) = db.counted_pool(25).await;
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (pool, c, ns) = (pool.clone(), s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { get(&pool, &c, &ns, 7).await.map_err(|e| e.to_string()) }));
        }
        for t in tasks {
            assert_eq!(t.await.expect("задача").expect("профиль").map(|p| p.name), Some("user7".to_string()));
        }
        assert_eq!(queries.get(), 1, "двадцать холодных читателей, один запрос к базе");
        let ttl = s.cmd(&["TTL", &s.key("profile:7")]).await.expect("TTL").int().unwrap_or(-1);
        assert!((240..360).contains(&ttl), "срок около 300 секунд с разбросом, TTL = {ttl}");
    }

    #[tokio::test]
    async fn readers_never_see_old_name_after_rename() {
        let db = TestDb::with_sql(SEED).await;
        let s = Scope::new().await;
        let c = s.client();
        for round in 0..5 {
            let name = format!("новое имя {round}");
            let mut readers = Vec::new();
            for _ in 0..10 {
                let (pool, c, ns) = (db.pool.clone(), s.client(), s.prefix().to_string());
                readers.push(tokio::spawn(async move { get(&pool, &c, &ns, 3).await.map_err(|e| e.to_string()) }));
            }
            rename(&db.pool, &c, s.prefix(), 3, &name).await.expect("переименование");
            for r in readers {
                r.await.expect("задача").expect("чтение");
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let now = get(&db.pool, &c, s.prefix(), 3).await.expect("чтение").expect("профиль");
            assert_eq!(now.name, name, "после переименования и окончания чтений кэш отдаёт новое имя");
        }
        let ttls: std::collections::BTreeSet<i64> = {
            let mut set = std::collections::BTreeSet::new();
            for id in 10..40 {
                get(&db.pool, &c, s.prefix(), id).await.expect("прогрев");
                set.insert(s.cmd(&["TTL", &s.key(&format!("profile:{id}"))]).await.expect("TTL").int().unwrap_or(-1));
            }
            set
        };
        assert!(ttls.len() >= 8, "сроки у разных профилей разбросаны: {ttls:?}");
    }
}
