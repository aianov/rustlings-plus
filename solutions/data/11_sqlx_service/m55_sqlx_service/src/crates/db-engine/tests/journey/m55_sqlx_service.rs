use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use db_engine::{
    DEFAULT_DB_URL, DbConfig, DbError, PG_URL_ENV, TxPolicy, connect, contains_pattern,
    drop_schema, escape_like, transact,
};

fn config(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_tx_{name}_{}", std::process::id())).pool(2)
}

const FLAKY: &str = "DO $$ BEGIN RAISE EXCEPTION 'сериализация' USING ERRCODE = '40001'; END $$";

#[test]
fn like_patterns_escape_wildcards() {
    assert_eq!(escape_like("100%_a\\b"), "100\\%\\_a\\\\b");
    assert_eq!(contains_pattern("a_b"), "%a\\_b%");
}

#[tokio::test]
async fn transactions_retry_conflicts_and_stop_at_the_limit() {
    let config = config("retry");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    let tries = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&tries);
    let value = transact(&pool, TxPolicy::default(), move |conn| {
        let counted = Arc::clone(&counted);
        Box::pin(async move {
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                sqlx::query(FLAKY).execute(&mut *conn).await?;
            }
            sqlx::query_scalar::<_, i32>("SELECT 7")
                .fetch_one(&mut *conn)
                .await
        })
    })
    .await
    .expect("второй заход проходит");
    assert_eq!(
        (value, tries.load(Ordering::SeqCst)),
        (7, 2),
        "40001 повторяется"
    );
    let always = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&always);
    let failed = transact(&pool, TxPolicy::default(), move |conn| {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            sqlx::query(FLAKY).execute(&mut *conn).await?;
            Ok(())
        })
    })
    .await;
    assert!(matches!(failed, Err(DbError::Conflict(..))), "{failed:?}");
    assert_eq!(always.load(Ordering::SeqCst), 3, "предел попыток");
    let policy = TxPolicy {
        attempts: 3,
        statement_timeout: Duration::from_millis(100),
    };
    let slow = transact(&pool, policy, |conn| {
        Box::pin(async move {
            sqlx::query("SELECT pg_sleep(1)")
                .execute(&mut *conn)
                .await?;
            Ok(())
        })
    })
    .await;
    assert!(
        matches!(slow, Err(DbError::Timeout(..))),
        "SET LOCAL statement_timeout: {slow:?}"
    );
    let _ = drop_schema(&config).await;
}
