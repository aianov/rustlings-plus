use sqlx::PgPool;

async fn befriend(pool: &PgPool, a: i64, b: i64) -> Result<(), sqlx::Error> {
    let (first, second) = if a < b { (a, b) } else { (b, a) };
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE users SET friends = friends + 1 WHERE id = $1").bind(first).execute(&mut *tx).await?;
    sqlx::query("SELECT pg_sleep(0.1)").execute(&mut *tx).await?;
    sqlx::query("UPDATE users SET friends = friends + 1 WHERE id = $1").bind(second).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn crossing_requests_do_not_deadlock() {
        let db = TestDb::with_sql("CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, friends integer NOT NULL DEFAULT 0); INSERT INTO users DEFAULT VALUES; INSERT INTO users DEFAULT VALUES;").await;
        let mut handles = Vec::new();
        for i in 0..6 {
            let pool = db.pool.clone();
            let (a, b) = if i % 2 == 0 { (1, 2) } else { (2, 1) };
            handles.push(tokio::spawn(async move { befriend(&pool, a, b).await }));
        }
        for h in handles {
            if let Err(e) = h.await.expect("задача упала") {
                panic!("befriend вернул ошибку {e}: встречные запросы блокируют друг друга (40P01), блокировки нужно брать в одном порядке");
            }
        }
        let t = db.table("SELECT id, friends FROM users ORDER BY id").await;
        assert_eq!(t.rows, [["1", "6"], ["2", "6"]], "каждый из шести вызовов прибавил обоим по единице:\n{t}");
    }
}
