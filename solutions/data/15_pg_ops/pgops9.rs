use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

async fn purge_temp(pool: &PgPool, table: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(&format!("DROP TABLE {table}")).execute(pool).await?;
    Ok(())
}

async fn connect_service(owner: &PgConnectOptions, role: &str, password: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(owner.clone().username(role).password(password))
        .await
        .expect("не удалось подключиться к базе")
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
    mod role_h {
        use super::*;
        pub use std::str::FromStr;
        pub use sqlx::postgres::PgConnectOptions;

        pub struct RoleGuard(pub String);

        impl Drop for RoleGuard {
            fn drop(&mut self) {
                let role = self.0.clone();
                let _ = std::thread::spawn(move || {
                    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                        return;
                    };
                    rt.block_on(async {
                        if let Ok(mut c) = PgConnection::connect(&course_support::pg::url()).await {
                            let _ = sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = $1").bind(&role).execute(&mut c).await;
                            let _ = sqlx::raw_sql(&format!("DROP OWNED BY \"{role}\"")).execute(&mut c).await;
                            let _ = sqlx::raw_sql(&format!("DROP ROLE IF EXISTS \"{role}\"")).execute(&mut c).await;
                        }
                    });
                })
                .join();
            }
        }

        pub fn as_role(role: &str, password: &str) -> PgConnectOptions {
            PgConnectOptions::from_str(&course_support::pg::url()).expect("адрес базы").username(role).password(password)
        }

        pub fn code_of(e: &sqlx::Error) -> String {
            match e {
                sqlx::Error::Database(d) => d.code().map(|c| c.to_string()).unwrap_or_default(),
                other => format!("не ошибка базы: {other}"),
            }
        }
    }
    use role_h::*;

    const PW: &str = "pw_service_secret";

    async fn world(db: &TestDb) -> (String, RoleGuard) {
        db.run("CREATE TABLE sessions (id bigserial PRIMARY KEY, user_id bigint NOT NULL, expires_at timestamptz NOT NULL); INSERT INTO sessions (user_id, expires_at) VALUES (1, now() + interval '1 day'), (2, now() + interval '2 days')").await;
        let role = course_support::unique_name("svc");
        let guard = RoleGuard(role.clone());
        let schema = db.schema();
        db.run(&format!(
            "CREATE ROLE \"{role}\" LOGIN PASSWORD '{PW}';
             GRANT USAGE ON SCHEMA \"{schema}\" TO \"{role}\";
             GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA \"{schema}\" TO \"{role}\";
             GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA \"{schema}\" TO \"{role}\""
        ))
        .await;
        (role, guard)
    }

    #[tokio::test]
    async fn service_runs_as_the_application_role() {
        let db = TestDb::new().await;
        let (role, _guard) = world(&db).await;
        let pool = connect_service(&db.options(), &role, PW).await;
        let who: String = sqlx::query_scalar("SELECT current_user::text").fetch_one(&pool).await.expect("current_user");
        assert_eq!(who, role, "сервис должен работать ролью приложения, а не владельцем схемы");
        sqlx::query("INSERT INTO sessions (user_id, expires_at) VALUES (3, now() + interval '1 hour')").execute(&pool).await.expect("запись через пул приложения");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions").fetch_one(&pool).await.expect("чтение через пул приложения");
        assert_eq!(n, 3, "обычная работа с данными не должна пострадать");
    }

    #[tokio::test]
    async fn the_faulty_job_can_no_longer_destroy_data() {
        let db = TestDb::new().await;
        let (role, _guard) = world(&db).await;
        let pool = connect_service(&db.options(), &role, PW).await;
        let res = purge_temp(&pool, "sessions").await;
        let err = res.expect_err("задание с ошибочным именем не должно было суметь удалить таблицу");
        assert_eq!(code_of(&err), "42501", "ожидалась ошибка прав (42501), а пришло: {err}");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions").fetch_one(&db.pool).await.expect("чтение владельцем");
        assert_eq!(n, 2, "таблица sessions должна остаться на месте со всеми строками");
    }
}
