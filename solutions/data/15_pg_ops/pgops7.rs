fn sizes() -> &'static str {
    "SELECT relname AS table_name, pg_table_size(relid) AS table_bytes, pg_indexes_size(relid) AS indexes_bytes
     FROM pg_stat_user_tables
     WHERE schemaname = current_schema()
     ORDER BY pg_total_relation_size(relid) DESC"
}

fn unused_indexes() -> &'static str {
    "SELECT s.indexrelname AS index_name, pg_relation_size(s.indexrelid) AS size_bytes
     FROM pg_stat_user_indexes s
     JOIN pg_index i ON i.indexrelid = s.indexrelid
     WHERE s.schemaname = current_schema()
       AND s.idx_scan = 0
       AND NOT i.indisunique
       AND NOT i.indisprimary
     ORDER BY size_bytes DESC"
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
        CREATE TABLE events (id bigint PRIMARY KEY, code text NOT NULL, kind int NOT NULL, n int NOT NULL, payload text NOT NULL);
        CREATE TABLE tiny (id int PRIMARY KEY);
        INSERT INTO events SELECT g, 'c' || g, g % 10, g, 'payload-' || md5(g::text) FROM generate_series(1, 20000) g;
        INSERT INTO tiny VALUES (1);
        CREATE UNIQUE INDEX events_code_key ON events (code);
        CREATE INDEX events_kind_idx ON events (kind);
        CREATE INDEX events_payload_idx ON events (payload);
        CREATE INDEX events_n_idx ON events (n);
        ANALYZE events;
    ";

    async fn used_db() -> (TestDb, PgConnection) {
        let db = TestDb::with_sql(SEED).await;
        let mut c = open(&db, "ops7-user").await;
        exec(&mut c, "SET enable_seqscan = off").await;
        exec(&mut c, "SET enable_bitmapscan = off").await;
        for _ in 0..3 {
            exec(&mut c, "SELECT count(*) FROM events WHERE kind = 3").await;
        }
        flush_stats(&mut c).await;
        (db, c)
    }

    #[tokio::test]
    async fn sizes_are_listed_heaviest_first() {
        let (db, _c) = used_db().await;
        let sql = format!("SELECT r.table_name::text, r.table_bytes::bigint, r.indexes_bytes::bigint FROM ({}) AS r", sizes());
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(&sql).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос sizes упал: {e}"));
        let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, vec!["events", "tiny"], "таблицы текущей схемы, самая тяжёлая первой: {rows:?}");
        let truth: (i64, i64) = sqlx::query_as("SELECT pg_table_size('events'), pg_indexes_size('events')").fetch_one(&db.pool).await.expect("размеры");
        assert_eq!((rows[0].1, rows[0].2), truth, "размер таблицы нужен вместе с TOAST и служебными файлами (pg_table_size), размер индексов суммарный (pg_indexes_size)");
    }

    #[tokio::test]
    async fn only_removable_unused_indexes_are_listed() {
        let (db, _c) = used_db().await;
        let mut rows: Vec<(String, i64)> = Vec::new();
        for _ in 0..100 {
            sqlx::query("SELECT pg_stat_clear_snapshot()").execute(&db.pool).await.expect("снимок статистики");
            let sql = format!("SELECT r.index_name::text, r.size_bytes::bigint FROM ({}) AS r", unused_indexes());
            rows = sqlx::query_as(&sql).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос unused_indexes упал: {e}"));
            if !rows.iter().any(|r| r.0 == "events_kind_idx") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, vec!["events_payload_idx", "events_n_idx"], "кандидаты на удаление: payload (самый большой) и n; kind использовался, первичные ключи и уникальный индекс по code удалять нельзя. Получено: {rows:?}");
        assert!(rows[0].1 > rows[1].1 && rows[1].1 > 0, "размеры в байтах должны быть настоящими и по убыванию: {rows:?}");
    }
}
