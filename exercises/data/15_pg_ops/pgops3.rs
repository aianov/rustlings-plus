// Сервис заходит в базу ролью приложения. Бывает, что запрос уходит в бесконечность (забыли условие,
// взяли блокировку), а бывает, что код открыл транзакцию и забыл про неё. Защита ставится на самой роли:
// запрос дольше 500 мс прерывается, соединение, которое простаивает в транзакции дольше секунды,
// закрывается. Другим ролям и самой базе эти настройки не принадлежат: мигратору как раз нужны долгие запросы.
// Имя роли и пароль приходят параметрами. Роль должна уметь входить в базу.
// TODO: Допиши `role_script`: скрипт, который создаёт роль и выставляет ей оба ограничения.

fn role_script(role: &str, password: &str) -> String {
    let _ = (role, password);
    "SELECT 1".to_string()
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

    #[tokio::test]
    async fn role_gets_both_limits() {
        let db = TestDb::new().await;
        let role = course_support::unique_name("app");
        let _guard = RoleGuard(role.clone());
        db.run(&role_script(&role, "pw_secret_1")).await;
        let mut c = PgConnection::connect_with(&as_role(&role, "pw_secret_1")).await.unwrap_or_else(|e| panic!("роль должна входить по паролю (LOGIN): {e}"));
        let st: String = sqlx::query_scalar("SHOW statement_timeout").fetch_one(&mut c).await.expect("SHOW");
        let idle: String = sqlx::query_scalar("SHOW idle_in_transaction_session_timeout").fetch_one(&mut c).await.expect("SHOW");
        assert_eq!((st.as_str(), idle.as_str()), ("500ms", "1s"), "у новой сессии роли должны быть statement_timeout = 500ms и idle_in_transaction_session_timeout = 1s");
    }

    #[tokio::test]
    async fn long_query_is_cancelled() {
        let db = TestDb::new().await;
        let role = course_support::unique_name("app");
        let _guard = RoleGuard(role.clone());
        db.run(&role_script(&role, "pw_secret_2")).await;
        let mut c = PgConnection::connect_with(&as_role(&role, "pw_secret_2")).await.expect("вход ролью");
        let started = std::time::Instant::now();
        let err = sqlx::query("SELECT pg_sleep(4)").execute(&mut c).await.expect_err("запрос на 4 секунды должен быть прерван по statement_timeout");
        assert_eq!(code_of(&err), "57014", "ожидалась отмена по таймауту (57014), а пришло: {err}");
        assert!(started.elapsed() < Duration::from_millis(2500), "прерывание должно случиться примерно через 500 мс, а прошло {:?}", started.elapsed());
        sqlx::query("SELECT 1").execute(&mut c).await.expect("после отмены запроса соединение остаётся рабочим");
    }

    #[tokio::test]
    async fn forgotten_transaction_is_closed() {
        let db = TestDb::new().await;
        let role = course_support::unique_name("app");
        let _guard = RoleGuard(role.clone());
        db.run(&role_script(&role, "pw_secret_3")).await;
        let mut c = PgConnection::connect_with(&as_role(&role, "pw_secret_3")).await.expect("вход ролью");
        exec(&mut c, "BEGIN").await;
        exec(&mut c, "SELECT 1").await;
        tokio::time::sleep(Duration::from_millis(1800)).await;
        assert!(sqlx::query("SELECT 1").execute(&mut c).await.is_err(), "транзакция простояла дольше секунды, сервер должен был закрыть соединение");
    }

    #[tokio::test]
    async fn owner_is_not_limited() {
        let db = TestDb::new().await;
        let role = course_support::unique_name("app");
        let _guard = RoleGuard(role.clone());
        db.run(&role_script(&role, "pw_secret_4")).await;
        let mut owner = PgConnection::connect(&course_support::pg::url()).await.expect("вход владельцем");
        let st: String = sqlx::query_scalar("SHOW statement_timeout").fetch_one(&mut owner).await.expect("SHOW");
        assert_eq!(st, "0", "ограничения принадлежат роли приложения: у остальных ролей statement_timeout остаётся 0 (без ограничения), а не {st}");
    }
}
