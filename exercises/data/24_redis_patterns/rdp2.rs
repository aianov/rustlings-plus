// Смена имени сбрасывает кэш профиля, потом обновляет базу. Иногда после смены имени все видят старое имя
// ещё пять минут. Причина в порядке: между сбросом кэша и коммитом читатель успевает промахнуться, прочитать
// из базы ещё старое значение (транзакция не закоммичена) и положить его обратно в кэш. Почини `rename` так,
// чтобы после её завершения кэш никогда не держал старое имя.

use course_support::redis::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;

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

async fn rename(db: &PgPool, c: &Client, ns: &str, id: i64, name: &str) -> Result<(), BoxError> {
    c.cmd(&["DEL", &format!("{ns}:profile:{id}")]).await?;
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE profiles SET name = $1 WHERE id = $2").bind(name).bind(id).execute(&mut *tx).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    tx.commit().await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE profiles (id bigint PRIMARY KEY, name text NOT NULL, bio text NOT NULL); INSERT INTO profiles VALUES (7, 'kate', 'пишу на Rust'), (9, 'bob', 'просто bob')";

    #[tokio::test]
    async fn no_stale_name_after_rename() {
        let db = TestDb::with_sql(SEED).await;
        let s = Scope::new().await;
        let c = s.client();
        get_profile(&db.pool, &c, s.prefix(), 7).await.expect("прогрев кэша");
        let (pool, writer, ns) = (db.pool.clone(), s.client(), s.prefix().to_string());
        let update = tokio::spawn(async move { rename(&pool, &writer, &ns, 7, "katherine").await.map_err(|e| e.to_string()) });
        tokio::time::sleep(Duration::from_millis(100)).await;
        get_profile(&db.pool, &c, s.prefix(), 7).await.expect("читатель посреди обновления");
        update.await.expect("задача").expect("переименование");
        let now = get_profile(&db.pool, &c, s.prefix(), 7).await.expect("чтение после").expect("профиль");
        assert_eq!(now.name, "katherine", "после переименования кэш не должен отдавать старое имя");
    }
}
