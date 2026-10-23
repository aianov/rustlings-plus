// Пересчёт статистики пользователя дорогой, и двух одновременных пересчётов одного пользователя
// быть не должно. `recompute(user_id)` увеличивает stats.runs этого пользователя (работа
// имитируется паузой) и возвращает true. Если пересчёт этого пользователя уже идёт, не ждёт, а
// возвращает false без работы. Пересчёты разных пользователей друг другу не мешают. Пользуйся
// механизмом блокировок «по смыслу», а не по строке таблицы.
// TODO: Допиши функцию.

use sqlx::PgPool;

async fn recompute(pool: &PgPool, user_id: i64) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE stats SET runs = runs + 1 WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("SELECT pg_sleep(0.3)").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SCHEMA: &str = "CREATE TABLE stats (user_id bigint PRIMARY KEY, runs integer NOT NULL DEFAULT 0); INSERT INTO stats (user_id) VALUES (18001), (18003), (18011), (18012), (18013), (18014);";

    #[tokio::test]
    async fn one_recompute_per_user() {
        let db = TestDb::with_sql(SCHEMA).await;
        let mut handles = Vec::new();
        for _ in 0..6 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move { recompute(&pool, 18001).await }));
        }
        let mut wins = 0;
        for h in handles {
            if h.await.expect("задача упала").expect("recompute") {
                wins += 1;
            }
        }
        assert_eq!(wins, 1, "пересчёт одного пользователя должен выполниться один раз, а выполнился {wins}");
        assert_eq!(db.table("SELECT runs FROM stats WHERE user_id = 18001").await.cell(0, 0), "1");
    }

    #[tokio::test]
    async fn different_users_do_not_block_each_other() {
        let db = TestDb::with_sql(SCHEMA).await;
        let mut holder = db.pool.begin().await.expect("держатель блокировки");
        sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(18011_i64).execute(&mut *holder).await.expect("блокировка");
        assert!(!recompute(&db.pool, 18011).await.expect("recompute"), "пользователь 18011 уже пересчитывается: ответ false без ожидания");
        assert!(recompute(&db.pool, 18012).await.expect("recompute"), "пересчёт другого пользователя не должен зависеть от чужой блокировки");
        holder.rollback().await.expect("откат");
        assert!(recompute(&db.pool, 18011).await.expect("recompute"), "после освобождения блокировки пересчёт возможен");
    }

    #[tokio::test]
    async fn lock_is_released_after_commit() {
        let db = TestDb::with_sql(SCHEMA).await;
        assert!(recompute(&db.pool, 18003).await.expect("recompute"));
        assert!(recompute(&db.pool, 18003).await.expect("recompute"), "после завершения первого пересчёта блокировка должна освободиться");
        assert_eq!(db.table("SELECT runs FROM stats WHERE user_id = 18003").await.cell(0, 0), "2");
    }
}
