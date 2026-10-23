use contracts::UserId;
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::repository::{AUTHOR_PAGE, BY_IDS, SearchQueries};
use post_service::{MIGRATIONS, PgPosts};
use sqlx::PgPool;
use sqlx::types::JsonValue;

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_index_{name}_{}", std::process::id())).pool(2)
}

const FILL: &str = "
INSERT INTO posts (id, author, author_name, body, at)
SELECT g, g % 100 + 1, 'автор', 'текст ' || g, g * 1000 FROM generate_series(1, 5000) g;
INSERT INTO likes (post, user_id, at)
SELECT g, g % 50 + 1, g FROM generate_series(1, 5000) g;
INSERT INTO post_tags (post, position, tag)
SELECT g, 0, 'tag' || (g % 40) FROM generate_series(1, 5000) g;
ANALYZE;
";

async fn plan(pool: &PgPool, query: &str, binds: &[JsonValue]) -> String {
    let sql = format!("EXPLAIN (FORMAT JSON) {query}");
    let mut explain = sqlx::query_scalar::<_, JsonValue>(&sql);
    for bind in binds {
        explain = match bind {
            JsonValue::Array(items) => explain.bind(
                items
                    .iter()
                    .filter_map(JsonValue::as_i64)
                    .collect::<Vec<i64>>(),
            ),
            other => explain.bind(other.as_i64()),
        };
    }
    explain.fetch_one(pool).await.expect("EXPLAIN").to_string()
}

#[tokio::test]
async fn key_queries_use_indexes_and_pages_are_keyset() {
    let config = database("plans");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    sqlx::raw_sql(FILL).execute(&pool).await.expect("данные");
    let author = plan(
        &pool,
        AUTHOR_PAGE,
        &[7.into(), i64::MAX.into(), i64::MAX.into(), 20.into()],
    )
    .await;
    assert!(
        !author.contains("Seq Scan"),
        "посты автора по индексу: {author}"
    );
    assert!(author.contains("posts_author_page"), "{author}");
    let hydrate = plan(
        &pool,
        BY_IDS,
        &[
            JsonValue::Array(vec![10.into(), 20.into(), 30.into()]),
            3.into(),
            JsonValue::Array(vec![99.into()]),
        ],
    )
    .await;
    assert!(
        !hydrate.contains("Seq Scan"),
        "гидрация по индексам: {hydrate}"
    );
    let posts = PgPosts::open(pool.clone(), 8).await.expect("репозиторий");
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = posts
            .author_page(UserId(8), before, 20)
            .await
            .expect("страница");
        if page.is_empty() {
            break;
        }
        before = page.last().map(|post| (post.at, post.id.0));
        seen.extend(page.into_iter().map(|post| post.id.0));
    }
    let mut expected: Vec<u64> = (1..=5000).filter(|id| id % 100 + 1 == 8).collect();
    expected.reverse();
    assert_eq!(seen, expected, "keyset (at, id) без пропусков и повторов");
    let _ = drop_schema(&config).await;
}
