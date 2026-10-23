// В тестах репозиториев каждому тесту нужна своя схема: параллельные тесты не должны видеть
// таблицы друг друга. Помощник `TestEnv::create(ddl)` создаёт схему с уникальным именем, выполняет в
// ней DDL и отдаёт пул, в котором ВСЕ соединения смотрят в эту схему (search_path). `drop_schema`
// закрывает пул и удаляет схему целиком. Адрес базы это `course_support::pg::url()`.
// TODO: Допиши оба метода. Подумай, что случится, если выставить search_path командой SET на
// одном соединении, а пул откроет второе.

use sqlx::PgPool;

struct TestEnv {
    pool: PgPool,
    schema: String,
}

impl TestEnv {
    async fn create(ddl: &str) -> TestEnv {
        let _ = ddl;
        todo!()
    }

    async fn drop_schema(self) {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn two_environments_do_not_see_each_other() {
        let ddl = "CREATE TABLE t (n int NOT NULL); CREATE TABLE u (n int NOT NULL)";
        let (a, b) = tokio::join!(TestEnv::create(ddl), TestEnv::create(ddl));
        assert_ne!(a.schema, b.schema, "схемы двух окружений должны иметь разные имена");
        sqlx::query("INSERT INTO t VALUES (1), (2), (3)").execute(&a.pool).await.expect("запись в окружение a");
        sqlx::query("INSERT INTO t VALUES (10)").execute(&b.pool).await.expect("запись в окружение b");
        for (env, expected) in [(&a, 3), (&b, 1)] {
            let mut tasks = Vec::new();
            for _ in 0..8 {
                let pool = env.pool.clone();
                tasks.push(tokio::spawn(async move {
                    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM t, pg_sleep(0.1)").fetch_one(&pool).await
                }));
            }
            for task in tasks {
                let n = task.await.unwrap().expect("каждое соединение пула должно видеть таблицу окружения: search_path нужен всем соединениям, а не одному");
                assert_eq!(n, expected, "окружение {}: в таблице t должно быть {expected} строк", env.schema);
            }
        }
        a.drop_schema().await;
        b.drop_schema().await;
    }

    #[tokio::test]
    async fn dropped_schema_is_gone() {
        use sqlx::Connection;
        let env = TestEnv::create("CREATE TABLE t (n int)").await;
        let name = env.schema.clone();
        let mut admin = sqlx::PgConnection::connect(&course_support::pg::url()).await.unwrap();
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_namespace WHERE nspname = $1").bind(&name).fetch_one(&mut admin).await.unwrap();
        assert_eq!(before, 1, "после create схема должна существовать");
        env.drop_schema().await;
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_namespace WHERE nspname = $1").bind(&name).fetch_one(&mut admin).await.unwrap();
        assert_eq!(after, 0, "после drop_schema схема должна исчезнуть вместе с таблицами");
    }
}
