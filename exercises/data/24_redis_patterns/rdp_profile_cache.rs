// Квиз главы: кэш профилей над Postgres, который не врёт.
// 1) `get(db, c, ns, id)`: cache-aside, ключ `<ns>:profile:<id>`, JSON, срок 300 секунд плюс разброс 0..59 по id;
//    при промахе пересчитывает один (замок `<ns>:profile:<id>:lock`, SET NX PX 2000), остальные ждут значение
//    в кэше, проверяя раз в 20 мс.
// 2) `rename(db, c, ns, id, name)`: обновление в базе и сброс кэша так, чтобы после возврата никто не читал
//    старое имя. Учти читателя, который прямо сейчас пересчитывает кэш по старым данным: он положит их уже
//    после твоего сброса.
// TODO: Напиши обе функции (структура Profile и подключения уже есть).

use course_support::redis::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Profile {
    name: String,
    bio: String,
}

async fn get(db: &PgPool, c: &Client, ns: &str, id: i64) -> Result<Option<Profile>, BoxError> {
    let _ = (db, c, ns, id);
    todo!()
}

async fn rename(db: &PgPool, c: &Client, ns: &str, id: i64, name: &str) -> Result<(), BoxError> {
    let _ = (db, c, ns, id, name);
    todo!()
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
