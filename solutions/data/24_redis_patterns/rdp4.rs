use course_support::redis::Client;
use sqlx::PgPool;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn compute(db: &PgPool, author: i64) -> Result<i64, BoxError> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM posts, pg_sleep(0.5) WHERE author = $1").bind(author).fetch_one(db).await?)
}

async fn posts_this_week(db: &PgPool, c: &Client, ns: &str, author: i64) -> Result<i64, BoxError> {
    let key = format!("{ns}:author:{author}:week");
    let lock = format!("{key}:lock");
    for _ in 0..150 {
        if let Some(n) = c.cmd(&["GET", &key]).await?.int() {
            return Ok(n);
        }
        if !c.cmd(&["SET", &lock, "1", "NX", "PX", "2000"]).await?.is_nil() {
            let n = compute(db, author).await?;
            c.cmd(&["SET", &key, &n.to_string(), "EX", "60"]).await?;
            c.cmd(&["DEL", &lock]).await?;
            return Ok(n);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("значение так и не появилось в кэше".into())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, author bigint NOT NULL); INSERT INTO posts SELECT g, CASE WHEN g % 3 = 0 THEN 7 ELSE 9 END FROM generate_series(1, 30) g";

    #[tokio::test]
    async fn only_one_reader_recomputes() {
        let db = TestDb::with_sql(SEED).await;
        let (pool, queries) = db.counted_pool(25).await;
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (pool, c, ns) = (pool.clone(), s.client(), s.prefix().to_string());
            tasks.push(tokio::spawn(async move { posts_this_week(&pool, &c, &ns, 7).await.map_err(|e| e.to_string()) }));
        }
        for t in tasks {
            assert_eq!(t.await.expect("задача").expect("счётчик"), 10, "у автора 7 десять постов");
        }
        assert_eq!(queries.get(), 1, "тяжёлый запрос выполнен один раз на двадцать читателей");
    }
}
