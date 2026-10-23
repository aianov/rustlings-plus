// Лайки поста это число в самом посте, и популярные посты получают тысячи лайков в минуту. Каждый лайк
// это UPDATE, а значит новая версия строки, и таблица раздувается быстрее, чем успевает vacuum. Есть
// дешёвый вид обновления: если новая версия помещается на ту же страницу и в строке не поменялся
// ни один проиндексированный столбец, индексы трогать не нужно (HOT-обновление). Здесь оба условия
// нарушены: по `likes` построен индекс для рейтинга, а страницы таблицы заполнены до краёв.
// Нужно, чтобы не менее 80% обновлений счётчика были HOT, а рейтинг «топ-3 по лайкам» по-прежнему работал.
// Данные при переезде не теряются. `bump` принимает `$1` (id поста), `top` отдаёт `id` и `likes`.
// TODO: Допиши `migrate` (изменение схемы и перенос данных) и поправь `bump` и `top` под новую схему.

fn migrate() -> &'static str {
    "SELECT 1"
}

fn bump() -> &'static str {
    "UPDATE posts SET likes = likes + 1 WHERE id = $1"
}

fn top() -> &'static str {
    "SELECT id, likes FROM posts ORDER BY likes DESC, id LIMIT 3"
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
        CREATE TABLE posts (id bigint PRIMARY KEY, author_id bigint NOT NULL, body text NOT NULL, likes int NOT NULL DEFAULT 0) WITH (autovacuum_enabled = false);
        CREATE INDEX posts_likes_idx ON posts (likes);
        INSERT INTO posts SELECT g, g % 50, repeat('x', 100), g % 7 FROM generate_series(1, 5000) g;
    ";

    async fn totals(db: &TestDb) -> (i64, i64) {
        sqlx::query("SELECT pg_stat_clear_snapshot()").execute(&db.pool).await.expect("снимок статистики");
        let row: (i64, i64) = sqlx::query_as("SELECT coalesce(sum(n_tup_upd), 0)::bigint, coalesce(sum(n_tup_hot_upd), 0)::bigint FROM pg_stat_user_tables WHERE schemaname = current_schema()")
            .fetch_one(&db.pool)
            .await
            .expect("статистика обновлений");
        row
    }

    async fn top_rows(db: &TestDb) -> Vec<(i64, i64)> {
        let sql = format!("SELECT r.id::bigint, r.likes::bigint FROM ({}) AS r", top());
        sqlx::query_as(&sql).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос top упал: {e}"))
    }

    #[tokio::test]
    async fn migration_keeps_the_ranking() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migrate()).await;
        assert_eq!(top_rows(&db).await, vec![(6, 6), (13, 6), (20, 6)], "после переезда лидеры те же: посты 6, 13, 20 с шестью лайками");
    }

    #[tokio::test]
    async fn counter_updates_are_mostly_hot() {
        let db = TestDb::with_sql(SEED).await;
        db.run(migrate()).await;
        let mut writer = open(&db, "ops6-writer").await;
        flush_stats(&mut writer).await;
        let before = totals(&db).await;
        let ids: Vec<i64> = (0..100).map(|k| 1 + 50 * k).collect();
        for id in &ids {
            for _ in 0..5 {
                sqlx::query(bump()).bind(*id).execute(&mut writer).await.unwrap_or_else(|e| panic!("запрос bump упал: {e}"));
            }
        }
        flush_stats(&mut writer).await;
        let mut delta = (0, 0);
        for _ in 0..100 {
            let now = totals(&db).await;
            delta = (now.0 - before.0, now.1 - before.1);
            if delta.0 >= 500 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(delta.0, 500, "статистика должна показать 500 обновлений, а показывает {}", delta.0);
        let share = delta.1 as f64 / delta.0 as f64;
        assert!(share >= 0.8, "HOT-обновлений должно быть не меньше 80%, а их {:.0}% ({} из {}). Проиндексированный столбец меняется? Есть ли в страницах свободное место?", share * 100.0, delta.1, delta.0);
        let mut expected: Vec<(i64, i64)> = (1..=5000i64).map(|id| (id, id % 7 + if ids.contains(&id) { 5 } else { 0 })).collect();
        expected.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        expected.truncate(3);
        assert_eq!(top_rows(&db).await, expected, "рейтинг после обновлений посчитан неверно");
    }
}
