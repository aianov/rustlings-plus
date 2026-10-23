// Дежурный пишет в чат: «база тормозит». Первое, на что смотрят: что сейчас делают соединения.
// Нужен запрос, который показывает «подозрительных»: соединения с запросом, который идёт дольше порога,
// и соединения, которые открыли транзакцию и ничего не делают, в том числе после ошибки внутри
// транзакции (у такого соединения свой статус). Обычные спящие соединения без транзакции и свежие
// запросы в список не попадают, как и само соединение, которое задаёт вопрос.
// Параметры: `$1` префикс имени приложения (`application_name`), `$2` порог в секундах (число).
// TODO: Допиши `long_sessions`: столбцы `application_name`, `state` и `seconds` (сколько секунд идёт
// транзакция или, если её начало неизвестно, запрос).

fn long_sessions() -> &'static str {
    "SELECT 1"
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

    #[tokio::test]
    async fn finds_long_queries_and_stuck_transactions() {
        let db = TestDb::new().await;
        let tag = new_tag();
        let _sessions = Sessions(tag.clone());
        let mut slow = open(&db, &format!("{tag}-slow")).await;
        let mut stuck = open(&db, &format!("{tag}-stuck")).await;
        let mut aborted = open(&db, &format!("{tag}-aborted")).await;
        let mut calm = open(&db, &format!("{tag}-calm")).await;
        exec(&mut stuck, "BEGIN").await;
        exec(&mut stuck, "SELECT 1").await;
        exec(&mut aborted, "BEGIN").await;
        let _ = sqlx::query("SELECT 1/0").execute(&mut aborted).await;
        exec(&mut calm, "SELECT 1").await;
        let _slow_task = tokio::spawn(async move {
            let _ = sqlx::query("SELECT pg_sleep(20)").execute(&mut slow).await;
        });
        tokio::time::sleep(Duration::from_millis(3600)).await;
        let mut fresh = open(&db, &format!("{tag}-fresh")).await;
        let _fresh_task = tokio::spawn(async move {
            let _ = sqlx::query("SELECT pg_sleep(20)").execute(&mut fresh).await;
        });
        wait_until(&db, "свежий запрос начал выполняться", &format!("SELECT count(*) = 1 FROM pg_stat_activity WHERE application_name = '{tag}-fresh' AND state = 'active'")).await;

        let sql = format!("SELECT r.application_name::text, r.state::text, r.seconds::float8 FROM ({}) AS r ORDER BY 1", long_sessions());
        let rows: Vec<(String, String, f64)> = sqlx::query_as(&sql).bind(&tag).bind(3.0f64).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос long_sessions упал: {e}"));
        let names: Vec<&str> = rows.iter().map(|r| r.0.trim_start_matches(tag.as_str())).collect();
        assert_eq!(names, vec!["-aborted", "-slow", "-stuck"], "должны быть три подозрительных: транзакция после ошибки, долгий запрос и забытая транзакция; свежий запрос, спящее соединение и сам вопрошающий не нужны. Получено: {rows:?}");
        let states: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(states, vec!["idle in transaction (aborted)", "active", "idle in transaction"], "статусы соединений определены неверно");
        assert!(rows.iter().all(|r| r.2 >= 3.0), "в seconds должно быть время с начала транзакции, не меньше порога: {rows:?}");
    }
}
