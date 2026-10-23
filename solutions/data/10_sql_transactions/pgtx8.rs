use sqlx::PgPool;

async fn attempt(pool: &PgPool, post_id: i64) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").execute(&mut *tx).await?;
    let views: i32 = sqlx::query_scalar("SELECT views FROM posts WHERE id = $1").bind(post_id).fetch_one(&mut *tx).await?;
    sqlx::query("SELECT pg_sleep(0.03)").execute(&mut *tx).await?;
    sqlx::query("UPDATE posts SET views = $2 WHERE id = $1").bind(post_id).bind(views + 1).execute(&mut *tx).await?;
    tx.commit().await

}

async fn bump_rr(pool: &PgPool, post_id: i64) -> Result<(), sqlx::Error> {
    for _ in 0..100 {
        match attempt(pool, post_id).await {
            Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("40001") => {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            other => return other,
        }
    }
    Err(sqlx::Error::PoolTimedOut)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn serialization_failures_are_retried() {
        let db = TestDb::with_sql("CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, views integer NOT NULL DEFAULT 0); INSERT INTO posts DEFAULT VALUES;").await;
        let mut handles = Vec::new();
        for _ in 0..12 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { bump_rr(&pool, 1).await }));
        }
        for h in handles {
            if let Err(e) = h.await.expect("задача упала") {
                panic!("bump_rr вернул ошибку {e}: при 40001 транзакцию нужно повторить целиком");
            }
        }
        assert_eq!(db.table("SELECT views FROM posts WHERE id = 1").await.cell(0, 0), "12", "двенадцать вызовов дают 12");
    }
}
