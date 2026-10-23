use sqlx::PgPool;

async fn migrate(pool: &PgPool, files: &[(i64, &str, &str)], lock_key: i64) -> Result<usize, sqlx::Error> {
    let mut lock = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock($1)").bind(lock_key).execute(&mut *lock).await?;
    let result = run_locked(pool, files).await;
    let unlock = sqlx::query("SELECT pg_advisory_unlock($1)").bind(lock_key).execute(&mut *lock).await;
    let applied = result?;
    unlock?;
    Ok(applied)
}

async fn run_locked(pool: &PgPool, files: &[(i64, &str, &str)]) -> Result<usize, sqlx::Error> {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS schema_migrations (version bigint PRIMARY KEY, name text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now())")
        .execute(pool)
        .await?;
    let done: Vec<i64> = sqlx::query_scalar("SELECT version FROM schema_migrations").fetch_all(pool).await?;
    let mut applied = 0;
    for (version, name, sql) in files {
        if done.contains(version) {
            continue;
        }
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(sql).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        applied += 1;
    }
    Ok(applied)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    async fn held(db: &TestDb, key: i64) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND granted AND ((classid::bigint << 32) | objid::bigint) = $1")
            .bind(key)
            .fetch_one(&db.pool)
            .await
            .unwrap()
    }

    fn files() -> Vec<(i64, &'static str, &'static str)> {
        vec![
            (1, "create_a", "CREATE TABLE a (id int); SELECT pg_sleep(0.3)"),
            (2, "create_b", "CREATE TABLE b (id int)"),
        ]
    }

    #[tokio::test]
    async fn exactly_one_copy_migrates() {
        let db = TestDb::new().await;
        let (f1, f2, f3, f4, f5) = (files(), files(), files(), files(), files());
        let results = tokio::join!(
            migrate(&db.pool, &f1, 5_700_004),
            migrate(&db.pool, &f2, 5_700_004),
            migrate(&db.pool, &f3, 5_700_004),
            migrate(&db.pool, &f4, 5_700_004),
            migrate(&db.pool, &f5, 5_700_004),
        );
        let mut total = 0;
        for result in [results.0, results.1, results.2, results.3, results.4] {
            match result {
                Ok(n) => total += n,
                Err(e) => panic!("одна из копий упала: {e}. Пока одна мигрирует, остальные должны ждать, а не повторять ту же работу"),
            }
        }
        assert_eq!(total, 2, "каждая миграция должна быть применена ровно один раз на все копии вместе");
        assert_eq!(held(&db, 5_700_004).await, 0, "после работы замок должен быть снят: он принадлежит сессии, которая вернулась в пул");
    }

    #[tokio::test]
    async fn lock_is_released_after_a_failure() {
        let db = TestDb::new().await;
        let broken = [(1, "broken", "CREATE TABLE a (id int); SELECT 1/0")];
        assert!(migrate(&db.pool, &broken, 5_700_005).await.is_err(), "деление на ноль должно прервать миграцию");
        assert_eq!(held(&db, 5_700_005).await, 0, "замок надо снимать и тогда, когда миграция упала, иначе следующий запуск зависнет навсегда");
        let ok = [(1, "fine", "CREATE TABLE a (id int)")];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), migrate(&db.pool, &ok, 5_700_005))
            .await
            .expect("повторный запуск завис на замке")
            .expect("повторный запуск");
        assert_eq!(n, 1, "после исправления миграция применяется");
    }
}
