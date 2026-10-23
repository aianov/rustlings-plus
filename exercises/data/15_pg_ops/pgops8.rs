// Приложение должно читать и писать данные, и больше ничего. Создавать, менять и удалять таблицы, чистить
// их командой TRUNCATE ему незачем, а баг или взломанный сервис с такими правами уничтожит базу. Миграции
// и таблицы принадлежат владельцу схемы, а приложение ходит отдельной ролью. Роль создаёт скрипт,
// который выполняет владелец. Приложению нужны все нынешние таблицы схемы, их автоматические
// счётчики (sequence), и все таблицы, которые владелец создаст в схеме потом: новая миграция не должна
// ломать сервис отсутствием прав.
// TODO: Допиши `app_role_sql`: создание роли с входом по паролю и права на схему `schema`.

fn app_role_sql(role: &str, password: &str, schema: &str) -> String {
    let _ = (role, password, schema);
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

    const SEED: &str = "
        CREATE TABLE posts (id bigserial PRIMARY KEY, body text NOT NULL);
        CREATE TABLE likes (post_id bigint NOT NULL, user_id bigint NOT NULL, PRIMARY KEY (post_id, user_id));
        INSERT INTO posts (body) VALUES ('первый');
    ";
    const PW: &str = "pw_app_secret";

    async fn app(db: &TestDb) -> (String, RoleGuard, PgConnection) {
        let role = course_support::unique_name("app");
        let guard = RoleGuard(role.clone());
        db.run(&app_role_sql(&role, PW, db.schema())).await;
        let options = as_role(&role, PW).options([("search_path", db.schema())]);
        let conn = PgConnection::connect_with(&options).await.unwrap_or_else(|e| panic!("роль приложения должна входить по паролю: {e}"));
        (role, guard, conn)
    }

    #[tokio::test]
    async fn app_can_work_with_data() {
        let db = TestDb::with_sql(SEED).await;
        let (_role, _guard, mut c) = app(&db).await;
        let id: i64 = sqlx::query_scalar("INSERT INTO posts (body) VALUES ('второй') RETURNING id").fetch_one(&mut c).await.unwrap_or_else(|e| panic!("вставка в таблицу с bigserial: {e} (нужны права и на счётчик)"));
        assert_eq!(id, 2, "второй пост");
        exec(&mut c, "UPDATE posts SET body = 'правка' WHERE id = 1").await;
        exec(&mut c, "INSERT INTO likes VALUES (1, 7)").await;
        exec(&mut c, "DELETE FROM likes").await;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM posts").fetch_one(&mut c).await.expect("чтение");
        assert_eq!(n, 2, "приложение читает свои данные");
    }

    #[tokio::test]
    async fn app_cannot_change_the_schema() {
        let db = TestDb::with_sql(SEED).await;
        let (role, _guard, mut c) = app(&db).await;
        for sql in ["CREATE TABLE evil (id int)", "DROP TABLE posts", "TRUNCATE posts", "ALTER TABLE posts ADD COLUMN extra int", "CREATE INDEX posts_body_idx ON posts (body)"] {
            let err = sqlx::query(sql).execute(&mut c).await.expect_err(&format!("роль приложения не должна уметь: {sql}"));
            assert_eq!(code_of(&err), "42501", "{sql}: ожидалась ошибка прав (42501), а пришло: {err}");
        }
        let flags: (bool, bool, bool) = sqlx::query_as("SELECT rolsuper, rolcreatedb, rolcreaterole FROM pg_roles WHERE rolname = $1").bind(&role).fetch_one(&db.pool).await.expect("флаги роли");
        assert_eq!(flags, (false, false, false), "роль приложения не суперпользователь и не создаёт роли и базы");
    }

    #[tokio::test]
    async fn tables_created_later_are_covered() {
        let db = TestDb::with_sql(SEED).await;
        let (_role, _guard, mut c) = app(&db).await;
        db.run("CREATE TABLE comments (id bigserial PRIMARY KEY, post_id bigint NOT NULL, body text NOT NULL)").await;
        let id: i64 = sqlx::query_scalar("INSERT INTO comments (post_id, body) VALUES (1, 'привет') RETURNING id").fetch_one(&mut c).await.unwrap_or_else(|e| panic!("таблица, созданная владельцем после выдачи прав, должна быть доступна приложению: {e}"));
        assert_eq!(id, 1, "первый комментарий");
        let body: String = sqlx::query_scalar("SELECT body FROM comments").fetch_one(&mut c).await.expect("чтение новой таблицы");
        assert_eq!(body, "привет", "чтение новой таблицы");
    }
}
