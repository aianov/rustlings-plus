fn holders() -> &'static str {
    "SELECT application_name, pid
     FROM pg_stat_activity
     WHERE application_name LIKE $1 || '%'
       AND pid <> pg_backend_pid()
       AND (backend_xmin IS NOT NULL OR backend_xid IS NOT NULL)"
}

fn evict() -> &'static str {
    "SELECT pg_terminate_backend(pid)
     FROM pg_stat_activity
     WHERE application_name LIKE $1 || '%'
       AND pid <> pg_backend_pid()
       AND (backend_xmin IS NOT NULL OR backend_xid IS NOT NULL)"
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

    struct Scene {
        tag: String,
        decoy: PgConnection,
        analytics: PgConnection,
        api: PgConnection,
    }

    async fn scene(db: &TestDb) -> Scene {
        let tag = new_tag();
        db.run("CREATE TABLE items (id int PRIMARY KEY, n int NOT NULL) WITH (autovacuum_enabled = false); INSERT INTO items SELECT g, 0 FROM generate_series(1, 5000) g").await;
        ensure_ext(db, "pgstattuple").await;
        let mut decoy = open(db, &format!("{tag}-decoy")).await;
        exec(&mut decoy, "BEGIN").await;
        exec(&mut decoy, "SELECT 1").await;
        let mut analytics = open(db, &format!("{tag}-analytics")).await;
        exec(&mut analytics, "BEGIN ISOLATION LEVEL REPEATABLE READ").await;
        exec(&mut analytics, "SELECT count(*) FROM items").await;
        let mut api = open(db, &format!("{tag}-api")).await;
        exec(&mut api, "SELECT 1").await;
        for _ in 0..3 {
            db.run("UPDATE items SET n = n + 1").await;
        }
        Scene { tag, decoy, analytics, api }
    }

    #[tokio::test]
    async fn only_the_snapshot_holder_is_named() {
        let db = TestDb::new().await;
        let s = scene(&db).await;
        let _sessions = Sessions(s.tag.clone());
        sqlx::raw_sql("VACUUM items").execute(&db.pool).await.expect("VACUUM");
        assert!(dead_tuples(&db, "items").await > 5000, "пока аналитик держит снимок, VACUUM не может убрать версии, обновлённые после него");
        let sql = format!("SELECT r.application_name::text FROM ({}) AS r ORDER BY 1", holders());
        let names: Vec<String> = sqlx::query_scalar(&sql).bind(&s.tag).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос holders упал: {e}"));
        assert_eq!(names, vec![format!("{}-analytics", s.tag)], "горизонт держит только аналитик со снимком repeatable read; сеанс, начавший транзакцию раньше, но ничего не державший, и обычное соединение не причастны");
    }

    #[tokio::test]
    async fn evicting_the_holder_lets_vacuum_finish() {
        let db = TestDb::new().await;
        let mut s = scene(&db).await;
        let _sessions = Sessions(s.tag.clone());
        sqlx::query(evict()).bind(&s.tag).execute(&db.pool).await.unwrap_or_else(|e| panic!("запрос evict упал: {e}"));
        assert!(sqlx::query("SELECT 1").execute(&mut s.analytics).await.is_err(), "сеанс аналитика должен быть закрыт");
        sqlx::query("SELECT 1").execute(&mut s.decoy).await.expect("сеанс без снимка ничего не держал, его закрывать нельзя");
        sqlx::query("SELECT 1").execute(&mut s.api).await.expect("обычное соединение закрывать нельзя");
        let mut left = i64::MAX;
        for _ in 0..20 {
            sqlx::raw_sql("VACUUM items").execute(&db.pool).await.expect("VACUUM");
            left = dead_tuples(&db, "items").await;
            if left == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        assert_eq!(left, 0, "без держателя горизонта VACUUM должен убрать все мёртвые версии");
    }
}
