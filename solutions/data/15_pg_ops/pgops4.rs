fn dead_report() -> &'static str {
    "SELECT relname AS table_name,
            n_dead_tup AS dead,
            round(100.0 * n_dead_tup / greatest(n_live_tup + n_dead_tup, 1))::int AS dead_pct
     FROM pg_stat_user_tables
     WHERE schemaname = current_schema()
       AND round(100.0 * n_dead_tup / greatest(n_live_tup + n_dead_tup, 1)) >= $1
     ORDER BY n_dead_tup DESC"
}

fn cleanup(table: &str) -> String {
    format!("VACUUM {table}")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    #[allow(dead_code, unused_imports)]
    mod ops_h {
        use super::*;
        pub use sqlx::{Connection, PgConnection};
        pub use std::time::Duration;

        pub fn new_tag() -> String {
            course_support::unique_name("ops").replace('_', "x")
        }

        pub async fn open(db: &TestDb, app: &str) -> PgConnection {
            PgConnection::connect_with(&db.options().application_name(app))
                .await
                .unwrap_or_else(|e| panic!("не удалось открыть соединение {app}: {e}"))
        }

        pub async fn exec(conn: &mut PgConnection, sql: &str) {
            sqlx::query(sql).execute(conn).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
        }

        pub struct Sessions(pub String);

        impl Drop for Sessions {
            fn drop(&mut self) {
                let tag = self.0.clone();
                let _ = std::thread::spawn(move || {
                    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                        return;
                    };
                    rt.block_on(async {
                        if let Ok(mut c) = PgConnection::connect(&course_support::pg::url()).await {
                            let _ = sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name LIKE $1 || '%' AND pid <> pg_backend_pid()")
                                .bind(&tag)
                                .execute(&mut c)
                                .await;
                        }
                    });
                })
                .join();
            }
        }

        pub async fn wait_until(db: &TestDb, what: &str, sql: &str) {
            for _ in 0..100 {
                let ok: bool = sqlx::query_scalar(sql).fetch_one(&db.pool).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
                if ok {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            panic!("за 5 секунд так и не наступило: {what}");
        }
    }
    use ops_h::*;

    #[allow(dead_code, unused_imports)]
    mod ext_h {
        use super::*;
        pub async fn ensure_ext(db: &TestDb, name: &str) {
            let mut conn = db.pool.acquire().await.expect("соединение");
            sqlx::raw_sql("SELECT pg_advisory_lock(15011)").execute(&mut *conn).await.expect("замок");
            let res = sqlx::raw_sql(&format!("CREATE EXTENSION IF NOT EXISTS {name} SCHEMA public")).execute(&mut *conn).await;
            sqlx::raw_sql("SELECT pg_advisory_unlock(15011)").execute(&mut *conn).await.expect("замок");
            res.unwrap_or_else(|e| panic!("расширение {name} должно ставиться: {e}"));
        }

        pub async fn dead_tuples(db: &TestDb, table: &str) -> i64 {
            sqlx::query_scalar("SELECT dead_tuple_count FROM pgstattuple($1::regclass)").bind(table).fetch_one(&db.pool).await.expect("pgstattuple")
        }

        pub async fn flush_stats(conn: &mut PgConnection) {
            exec(conn, "SELECT pg_stat_force_next_flush()").await;
            exec(conn, "SELECT 1").await;
        }
    }
    use ext_h::*;

    const SEED: &str = "
        CREATE TABLE feed_items (id int PRIMARY KEY, n int NOT NULL) WITH (autovacuum_enabled = false);
        CREATE TABLE clean_items (id int PRIMARY KEY, n int NOT NULL) WITH (autovacuum_enabled = false);
        CREATE TABLE empty_items (id int PRIMARY KEY) WITH (autovacuum_enabled = false);
        INSERT INTO feed_items SELECT g, 0 FROM generate_series(1, 20000) g;
        INSERT INTO clean_items SELECT g, 0 FROM generate_series(1, 20000) g;
    ";

    async fn bloated_db() -> (TestDb, PgConnection) {
        let db = TestDb::with_sql(SEED).await;
        ensure_ext(&db, "pgstattuple").await;
        let mut writer = open(&db, "ops4-writer").await;
        exec(&mut writer, "ANALYZE feed_items, clean_items, empty_items").await;
        exec(&mut writer, "UPDATE feed_items SET n = n + 1").await;
        exec(&mut writer, "UPDATE feed_items SET n = n + 1").await;
        flush_stats(&mut writer).await;
        (db, writer)
    }

    async fn report(db: &TestDb, pct: i32) -> Vec<(String, i64, i32)> {
        let sql = format!("SELECT r.table_name::text, r.dead::bigint, r.dead_pct::int FROM ({}) AS r", dead_report());
        sqlx::query_as(&sql).bind(pct).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос dead_report упал: {e} (пустая таблица: нет ли деления на ноль?)"))
    }

    async fn report_until(db: &TestDb, pct: i32, want: usize) -> Vec<(String, i64, i32)> {
        let mut rows = Vec::new();
        for _ in 0..100 {
            sqlx::query("SELECT pg_stat_clear_snapshot()").execute(&db.pool).await.expect("снимок статистики");
            rows = report(db, pct).await;
            if rows.len() == want {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        rows
    }

    #[tokio::test]
    async fn report_shows_only_bloated_tables() {
        let (db, _writer) = bloated_db().await;
        let rows = report_until(&db, 20, 1).await;
        let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, vec!["feed_items"], "порог 20 процентов проходит одна таблица: чистая clean_items и пустая empty_items в отчёт не входят. Получено: {rows:?}");
        let (_, dead, pct) = &rows[0];
        assert!(*dead > 10_000, "мёртвых версий должно быть много, а в отчёте {dead}");
        assert!((30..=75).contains(pct), "доля мёртвых от всех версий должна быть заметной, а в отчёте {pct}");
        assert!(report(&db, 99).await.is_empty(), "при пороге 99 процентов отчёт должен быть пустым");
    }

    #[tokio::test]
    async fn cleanup_removes_dead_versions_without_blocking_readers() {
        let (db, _writer) = bloated_db().await;
        assert!(dead_tuples(&db, "feed_items").await > 10_000, "перед очисткой мёртвых версий должно быть много");
        let tag = new_tag();
        let _sessions = Sessions(tag.clone());
        let mut reader = open(&db, &format!("{tag}-reader")).await;
        exec(&mut reader, "BEGIN").await;
        exec(&mut reader, "SELECT count(*) FROM feed_items").await;
        let mut vacuum = open(&db, &format!("{tag}-vacuum")).await;
        exec(&mut vacuum, "SET lock_timeout = '2s'").await;
        let mut left = i64::MAX;
        for _ in 0..10 {
            sqlx::raw_sql(&cleanup("feed_items")).execute(&mut vacuum).await.unwrap_or_else(|e| panic!("очистка упала или ждала читателя: {e} (эксклюзивные блокировки здесь не годятся)"));
            left = dead_tuples(&db, "feed_items").await;
            if left == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        exec(&mut reader, "ROLLBACK").await;
        assert_eq!(left, 0, "после очистки мёртвых версий не должно остаться");
        let rows = report_until(&db, 20, 0).await;
        assert!(rows.is_empty(), "после очистки таблица не должна попадать в отчёт: {rows:?}");
    }
}
