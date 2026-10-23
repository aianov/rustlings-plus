use contracts::{Cause, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, connect, drop_schema, migrate};
use post_service::common::now_micros;
use post_service::repository::PostRepository;
use post_service::{Draft, MIGRATIONS, PgPosts, PostText};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_body_{name}_{}", std::process::id())).pool(2)
}

fn upto(name: &str) -> usize {
    MIGRATIONS
        .iter()
        .position(|(file, _)| *file == name)
        .expect("миграция есть")
}

const OLD_INSERT: &str = "INSERT INTO posts (id, author, author_name, text, at) SELECT g, 1, 'alice', 'старый ' || g, g FROM generate_series($1::bigint, $2::bigint) g";

#[tokio::test]
async fn text_becomes_body_by_expand_and_contract() {
    let config = database("rename");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS[..upto("0005_body_expand.sql")])
        .await
        .expect("старая схема");
    sqlx::query(OLD_INSERT)
        .bind(1i64)
        .bind(2_500i64)
        .execute(&pool)
        .await
        .expect("старый код пишет text");
    migrate(&pool, &MIGRATIONS[..=upto("0006_body_backfill.sql")])
        .await
        .expect("expand и backfill");
    let empty: i64 = sqlx::query_scalar("SELECT count(*) FROM posts WHERE body IS NULL")
        .fetch_one(&pool)
        .await
        .expect("счёт");
    assert_eq!(empty, 0, "backfill пачками перенёс всё");
    sqlx::query(OLD_INSERT)
        .bind(2_501i64)
        .bind(2_501i64)
        .execute(&pool)
        .await
        .expect("старый код работает после expand");
    sqlx::query("INSERT INTO posts (id, author, author_name, body, at) VALUES (2502, 1, 'alice', 'новый', 1)")
        .execute(&pool)
        .await
        .expect("новый код пишет body");
    let pair: (String, String) = sqlx::query_as("SELECT text, body FROM posts WHERE id = 2501")
        .fetch_one(&pool)
        .await
        .expect("строка");
    assert_eq!(pair.0, pair.1, "триггер держит обе колонки");
    let pair: (String, String) = sqlx::query_as("SELECT text, body FROM posts WHERE id = 2502")
        .fetch_one(&pool)
        .await
        .expect("строка");
    assert_eq!(pair, (String::from("новый"), String::from("новый")));
    migrate(&pool, &MIGRATIONS)
        .await
        .expect("NOT NULL и contract");
    let gone: Option<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'text'",
    )
    .fetch_optional(&pool)
    .await
    .expect("каталог");
    assert_eq!(gone, None, "contract убрал text");
    let missing =
        sqlx::query("INSERT INTO posts (id, author, author_name, at) VALUES (9999, 1, 'a', 1)")
            .execute(&pool)
            .await;
    assert!(missing.is_err(), "body NOT NULL");
    let posts = PgPosts::open(pool.clone(), 9).await.expect("репозиторий");
    let fresh = posts
        .insert(
            UserId(1),
            String::from("alice"),
            Draft {
                text: PostText::try_from("после переезда").expect("текст"),
                tags: Vec::new(),
                links: Vec::new(),
            },
            &Cause::new(&Origin::new("m57"), now_micros()),
        )
        .await
        .expect("пост");
    assert_eq!(
        posts.get(fresh.id).await.map(|post| post.text),
        Some(String::from("после переезда"))
    );
    let _ = drop_schema(&config).await;
}
