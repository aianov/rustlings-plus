use std::str::FromStr;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

const SCHEMA: &str = include_str!("schema.sql");

async fn apply_schema(pool: &PgPool, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sql).execute(pool).await?;
    Ok(())
}

async fn connect_in_schema(url: &str, schema: &str, sql: &str) -> Result<PgPool, sqlx::Error> {
    let bootstrap = PgPool::connect(url).await?;
    sqlx::raw_sql(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"")).execute(&bootstrap).await?;
    bootstrap.close().await;
    let options = PgConnectOptions::from_str(url)?.options([("search_path", schema)]);
    let pool = PgPoolOptions::new().max_connections(8).connect_with(options).await?;
    apply_schema(&pool, sql).await?;
    Ok(pool)
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
