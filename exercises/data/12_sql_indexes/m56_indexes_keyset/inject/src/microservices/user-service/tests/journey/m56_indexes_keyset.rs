use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use sqlx::types::JsonValue;
use user_service::MIGRATIONS;
use user_service::repository::{FOLLOWERS, SELECT_BY_NAME};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    let schema = format!("t_uindex_{name}_{}", std::process::id());
    DbConfig::new(&url, &schema).pool(2)
}

const FILL: &str = "
INSERT INTO users (username) SELECT 'user' || g FROM generate_series(1, 5000) g;
INSERT INTO follows (follower, followee)
SELECT f, (f * k) % 5000 + 1 FROM generate_series(1, 5000) f, generate_series(2, 6) k
WHERE (f * k) % 5000 + 1 <> f
ON CONFLICT DO NOTHING;
ANALYZE;
";

#[tokio::test]
async fn followers_and_nick_lookup_use_indexes() {
    let config = database("plans");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    sqlx::raw_sql(FILL).execute(&pool).await.expect("данные");
    let followers: JsonValue = sqlx::query_scalar(&format!("EXPLAIN (FORMAT JSON) {FOLLOWERS}"))
        .bind(42i64)
        .bind(0i64)
        .bind(20i64)
        .fetch_one(&pool)
        .await
        .expect("EXPLAIN");
    let followers = followers.to_string();
    assert!(
        !followers.contains("Seq Scan"),
        "подписчики по индексу: {followers}"
    );
    let by_name: JsonValue = sqlx::query_scalar(&format!("EXPLAIN (FORMAT JSON) {SELECT_BY_NAME}"))
        .bind("User42")
        .fetch_one(&pool)
        .await
        .expect("EXPLAIN");
    let by_name = by_name.to_string();
    assert!(
        !by_name.contains("Seq Scan"),
        "ник по индексу lower(): {by_name}"
    );
    let _ = drop_schema(&config).await;
}
