// Сервис при старте сам разворачивает свою схему: читает файл schema.sql (он лежит рядом и
// подключён константой) и выполняет его. В файле несколько команд, комментарии и даже точка с
// запятой внутри строки, поэтому резать текст по `;` нельзя. Файл рассчитан на повторный запуск
// (IF NOT EXISTS). У каждого сервиса своя схема Postgres: все соединения его пула должны смотреть
// в неё, а не в public.
//  - `apply_schema(pool, sql)`: выполнить скрипт. Если в нём ошибка, не должно остаться
//    ничего из того, что шло до неё;
//  - `connect_in_schema(url, schema, sql)`: создать схему, если её нет, открыть пул, каждое
//    соединение которого видит эту схему первой, и применить скрипт.
// TODO: Реализуй обе функции.

use sqlx::PgPool;

const SCHEMA: &str = include_str!("schema.sql");

async fn apply_schema(pool: &PgPool, sql: &str) -> Result<(), sqlx::Error> {
    todo!()
}

async fn connect_in_schema(url: &str, schema: &str, sql: &str) -> Result<PgPool, sqlx::Error> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn schema_applies_twice() {
        let db = TestDb::new().await;
        apply_schema(&db.pool, SCHEMA).await.expect("схема должна применяться");
        apply_schema(&db.pool, SCHEMA).await.expect("файл рассчитан на повторный запуск");
        db.run("INSERT INTO subscribers (address) VALUES ('a@x.org')").await;
        let t = db.table("SELECT channel FROM subscribers").await;
        assert_eq!(t.rows, [["email; push"]], "значение по умолчанию со знаком ; внутри должно уцелеть:\n{t}");
    }

    #[tokio::test]
    async fn broken_script_leaves_no_trace() {
        let db = TestDb::new().await;
        let res = apply_schema(&db.pool, "CREATE TABLE ok_part (id int); CREATE TABEL broken (id int)").await;
        assert!(res.is_err(), "опечатка в скрипте должна дать ошибку");
        let t = db.table("SELECT to_regclass('ok_part') IS NULL").await;
        assert_eq!(t.cell(0, 0), "true", "таблица из первой, правильной команды не должна остаться: скрипт применяется целиком или никак");
    }

    #[tokio::test]
    async fn every_connection_sees_the_service_schema() {
        let mut db = TestDb::new().await;
        let schema = course_support::unique_name("svc");
        db.track_schema(&schema);
        let pool = connect_in_schema(&course_support::pg::url(), &schema, SCHEMA).await.expect("пул в схеме сервиса должен подняться");
        let mut held = Vec::new();
        for _ in 0..5 {
            let mut conn = pool.acquire().await.expect("соединение из пула");
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM subscribers").fetch_one(&mut *conn).await.expect("каждое соединение должно видеть таблицу subscribers без префикса схемы");
            assert_eq!(n, 0, "таблица новая и пустая");
            held.push(conn);
        }
        let in_schema: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema::text = $1")
            .bind(&schema)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(in_schema, 2, "обе таблицы сервиса должны лежать в его схеме");
        let in_public: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'subscribers'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(in_public, 0, "в public ничего создаваться не должно");
        drop(held);
        pool.close().await;
    }
}
