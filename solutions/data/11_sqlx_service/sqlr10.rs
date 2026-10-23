use sqlx::PgPool;

#[derive(Debug, PartialEq)]
enum LimitError {
    Timeout,
    Other(String),
}

async fn count_with_limit(pool: &PgPool, limit_ms: i64, sql: &str) -> Result<i64, LimitError> {
    let other = |e: sqlx::Error| LimitError::Other(e.to_string());
    let mut tx = pool.begin().await.map_err(other)?;
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(limit_ms.to_string())
        .execute(&mut *tx)
        .await
        .map_err(other)?;
    match sqlx::query_scalar::<_, i64>(sql).fetch_one(&mut *tx).await {
        Ok(value) => {
            tx.commit().await.map_err(other)?;
            Ok(value)
        }
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("57014") => Err(LimitError::Timeout),
        Err(e) => Err(other(e)),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    async fn single_pool(db: &TestDb) -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new().max_connections(1).connect_with(db.options()).await.unwrap()
    }

    #[tokio::test]
    async fn slow_query_is_stopped_by_the_server() {
        let db = TestDb::with_sql(include_str!("seed.sql")).await;
        let pool = single_pool(&db).await;
        let res = count_with_limit(&pool, 200, "SELECT count(*) FROM posts, pg_sleep(1.5)").await;
        assert_eq!(res, Err(LimitError::Timeout), "запрос на 1.5 секунды при лимите 200 мс должен закончиться Timeout (SQLSTATE 57014)");
    }

    #[tokio::test]
    async fn fast_query_passes_and_errors_are_not_timeouts() {
        let db = TestDb::with_sql(include_str!("seed.sql")).await;
        let pool = single_pool(&db).await;
        assert_eq!(count_with_limit(&pool, 5000, "SELECT count(*) FROM posts").await, Ok(8), "быстрый запрос должен вернуть число постов");
        match count_with_limit(&pool, 5000, "SELECT count(*) FROM no_such_table").await {
            Err(LimitError::Other(msg)) => assert!(msg.contains("no_such_table"), "в тексте ошибки должно быть имя таблицы: {msg}"),
            other => panic!("ошибка запроса не про таймаут должна быть Other, а пришло {other:?}"),
        }
    }

    #[tokio::test]
    async fn limit_does_not_leak_into_the_pooled_connection() {
        let db = TestDb::with_sql(include_str!("seed.sql")).await;
        let pool = single_pool(&db).await;
        let _ = count_with_limit(&pool, 200, "SELECT count(*) FROM posts, pg_sleep(1.5)").await;
        let after_timeout: String = sqlx::query_scalar("SHOW statement_timeout").fetch_one(&pool).await.unwrap();
        assert_eq!(after_timeout, "0", "после таймаута лимит остался на соединении пула: любой следующий запрос сервиса получит чужой лимит. Нужна область действия «до конца транзакции»");
        let _ = count_with_limit(&pool, 3000, "SELECT count(*) FROM posts").await;
        let after_ok: String = sqlx::query_scalar("SHOW statement_timeout").fetch_one(&pool).await.unwrap();
        assert_eq!(after_ok, "0", "после успешного запроса лимит тоже должен исчезнуть");
    }
}
