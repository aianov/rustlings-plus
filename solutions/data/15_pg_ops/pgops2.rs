fn blockers() -> &'static str {
    "SELECT w.application_name AS waiter, b.application_name AS blocker
     FROM pg_stat_activity w
     CROSS JOIN LATERAL unnest(pg_blocking_pids(w.pid)) AS bp(pid)
     JOIN pg_stat_activity b ON b.pid = bp.pid
     WHERE w.application_name LIKE $1 || '%'"
}

fn unblock() -> &'static str {
    "SELECT pg_terminate_backend(s.pid)
     FROM (SELECT DISTINCT unnest(pg_blocking_pids(pid)) AS pid FROM pg_stat_activity WHERE application_name LIKE $1 || '%') s"
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

    struct Scene {
        tag: String,
        holder: PgConnection,
        bystander: PgConnection,
        waiter: tokio::task::JoinHandle<Result<(), sqlx::Error>>,
    }

    async fn scene(db: &TestDb) -> Scene {
        let tag = new_tag();
        db.run("CREATE TABLE accounts (id int PRIMARY KEY, balance int NOT NULL); INSERT INTO accounts VALUES (1, 100)").await;
        let mut holder = open(db, &format!("{tag}-holder")).await;
        exec(&mut holder, "BEGIN").await;
        exec(&mut holder, "UPDATE accounts SET balance = balance + 1 WHERE id = 1").await;
        let mut bystander = open(db, &format!("{tag}-bystander")).await;
        exec(&mut bystander, "BEGIN").await;
        exec(&mut bystander, "SELECT 1").await;
        let mut waiting = open(db, &format!("{tag}-waiter")).await;
        let waiter = tokio::spawn(async move { sqlx::query("UPDATE accounts SET balance = balance + 10 WHERE id = 1").execute(&mut waiting).await.map(|_| ()) });
        wait_until(db, "ждущий встал на замок", &format!("SELECT count(*) = 1 FROM pg_stat_activity WHERE application_name = '{tag}-waiter' AND wait_event_type = 'Lock'")).await;
        Scene { tag, holder, bystander, waiter }
    }

    #[tokio::test]
    async fn shows_who_waits_for_whom() {
        let db = TestDb::new().await;
        let s = scene(&db).await;
        let _sessions = Sessions(s.tag.clone());
        let sql = format!("SELECT r.waiter::text, r.blocker::text FROM ({}) AS r", blockers());
        let rows: Vec<(String, String)> = sqlx::query_as(&sql).bind(&s.tag).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос blockers упал: {e}"));
        let expected = vec![(format!("{}-waiter", s.tag), format!("{}-holder", s.tag))];
        assert_eq!(rows, expected, "ждёт один сеанс и держит один; сторонний сеанс с открытой транзакцией замка не держит");
    }

    #[tokio::test]
    async fn unblock_removes_the_holder_and_nobody_else() {
        let db = TestDb::new().await;
        let mut s = scene(&db).await;
        let _sessions = Sessions(s.tag.clone());
        sqlx::query(unblock()).bind(&s.tag).execute(&db.pool).await.unwrap_or_else(|e| panic!("запрос unblock упал: {e}"));
        let done = tokio::time::timeout(Duration::from_secs(5), s.waiter).await;
        match done {
            Err(_) => panic!("ждущий так и не получил замок за 5 секунд: держатель сидит в открытой транзакции без запроса, pg_cancel_backend ему нечего отменять"),
            Ok(joined) => assert!(joined.expect("задача").is_ok(), "ждущий должен выполнить своё обновление, а не быть закрытым вместо виновника"),
        }
        assert!(sqlx::query("SELECT 1").execute(&mut s.holder).await.is_err(), "соединение виновника должно быть закрыто");
        sqlx::query("SELECT 1").execute(&mut s.bystander).await.expect("посторонний сеанс не причастен к затору и должен остаться жив");
        let balance: i32 = sqlx::query_scalar("SELECT balance FROM accounts WHERE id = 1").fetch_one(&db.pool).await.expect("баланс");
        assert_eq!(balance, 110, "изменение закрытого держателя откатилось (+1 нет), изменение ждавшего применилось (+10)");
    }
}
