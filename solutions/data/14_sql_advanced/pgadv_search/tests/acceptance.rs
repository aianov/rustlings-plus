use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use search_kit::{SCHEMA, find_people, search_posts};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

fn pg_url() -> String {
    std::env::var("RUSTLINGS_PG_URL").unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string())
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Env {
    pool: PgPool,
    schema: String,
}

impl Env {
    async fn new() -> Env {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let schema = format!("srch_{}_{}_{}", std::process::id(), n, nanos);
        let mut conn = PgConnection::connect(&pg_url())
            .await
            .unwrap_or_else(|e| panic!("Postgres курса не отвечает ({}): {e}. Запусти docker compose up -d", pg_url()));
        sqlx::raw_sql("SELECT pg_advisory_lock(14012)").execute(&mut conn).await.unwrap();
        let ext = sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA public").execute(&mut conn).await;
        sqlx::raw_sql("SELECT pg_advisory_unlock(14012)").execute(&mut conn).await.unwrap();
        ext.expect("расширение pg_trgm должно ставиться");
        sqlx::raw_sql(&format!("CREATE SCHEMA \"{schema}\"")).execute(&mut conn).await.unwrap();
        let options = PgConnectOptions::from_str(&pg_url())
            .unwrap()
            .options([("search_path", format!("{schema},public").as_str()), ("timezone", "UTC")]);
        let pool = PgPoolOptions::new().max_connections(4).acquire_timeout(Duration::from_secs(8)).connect_with(options).await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE users (id bigserial PRIMARY KEY, username text NOT NULL);
             CREATE TABLE posts (id bigint PRIMARY KEY, title text NOT NULL, body text NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(SCHEMA).execute(&pool).await.unwrap_or_else(|e| panic!("src/schema.sql не выполняется: {e}"));
        Env { pool, schema }
    }

    async fn run(&self, sql: &str) {
        sqlx::raw_sql(sql).execute(&self.pool).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
    }

    async fn rows(&self, sql: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>(sql).fetch_all(&self.pool).await.unwrap_or_else(|e| panic!("{sql}: {e}"))
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let schema = self.schema.clone();
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            rt.block_on(async {
                if let Ok(mut conn) = PgConnection::connect(&pg_url()).await {
                    let _ = sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE")).execute(&mut conn).await;
                }
            });
        })
        .join();
    }
}

async fn posts_env() -> Env {
    let env = Env::new().await;
    env.run(
        "INSERT INTO posts (id, title, body) VALUES
            (1, 'Котик', 'просто текст про погоду'),
            (2, 'Погода', 'котик гуляет, котик спит, котик ест'),
            (3, 'Про собак', 'проходили котики мимо и собака тоже'),
            (4, 'Без темы', 'ни о чём'),
            (5, 'Длинная история', repeat('слово ', 30) || 'котик ' || repeat('слово ', 30)),
            (6, 'Rust и Go', 'сравниваем rust и go на примере чата'),
            (7, 'Опасный', 'Том & Джерри <img src=x onerror=alert(1) котик')",
    )
    .await;
    env
}

fn ids(hits: &[search_kit::PostHit]) -> Vec<i64> {
    hits.iter().map(|h| h.id).collect()
}

#[tokio::test]
async fn schema_has_a_stored_vector_and_both_indexes() {
    let env = Env::new().await;
    let col = env
        .rows("SELECT data_type || '/' || is_generated FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name = 'search'")
        .await;
    assert_eq!(col, vec!["tsvector/ALWAYS"], "posts.search должен быть вычисляемым столбцом tsvector");
    let defs = env.rows("SELECT indexdef FROM pg_indexes WHERE schemaname = current_schema() AND tablename = 'posts' AND indexname NOT LIKE '%pkey'").await;
    assert!(defs.iter().any(|d| d.contains("USING gin") && d.contains("(search)")), "на posts.search нужен GIN-индекс, сейчас: {defs:?}");
    let defs = env.rows("SELECT indexdef FROM pg_indexes WHERE schemaname = current_schema() AND tablename = 'users' AND indexname NOT LIKE '%pkey'").await;
    assert!(defs.iter().any(|d| d.contains("trgm_ops") && d.contains("username")), "на users.username нужен триграммный индекс (gin_trgm_ops или gist_trgm_ops), сейчас: {defs:?}");
}

#[tokio::test]
async fn query_syntax_and_morphology() {
    let env = posts_env().await;
    let got = search_posts(&env.pool, "котики", 10).await.expect("search_posts");
    let mut found = ids(&got);
    found.sort();
    assert_eq!(found, vec![1, 2, 3, 5, 7], "слово «котики» находит все формы: котик, котики");
    let got = search_posts(&env.pool, "котик -собака", 10).await.expect("search_posts");
    assert!(!ids(&got).contains(&3), "пост 3 содержит «собака» и должен быть исключён знаком минус: {:?}", ids(&got));
    let got = search_posts(&env.pool, "rust or go", 10).await.expect("search_posts");
    assert_eq!(ids(&got), vec![6], "or даёт объединение, пост 6 один");
    let got = search_posts(&env.pool, "\"гуляет котик\"", 10).await.expect("search_posts");
    assert_eq!(ids(&got), vec![2], "в кавычках слова подряд: только в посте 2 «гуляет» стоит прямо перед «котик»");
    let got = search_posts(&env.pool, "\"спит гуляет\"", 10).await.expect("search_posts");
    assert!(got.is_empty(), "слова есть в посте 2, но не подряд и в другом порядке: {:?}", ids(&got));
}

#[tokio::test]
async fn title_outranks_body_and_limit_works() {
    let env = posts_env().await;
    let got = search_posts(&env.pool, "котик", 10).await.expect("search_posts");
    assert_eq!(ids(&got)[..3], [1, 2, 3], "пост 1 (слово в заголовке) выше поста 2 (три раза в тексте), пост 3 с одним упоминанием дальше: {:?}", ids(&got));
    let got = search_posts(&env.pool, "котик", 2).await.expect("search_posts");
    assert_eq!(ids(&got), vec![1, 2], "limit = 2 оставляет два лучших");
}

#[tokio::test]
async fn snippets_are_short_marked_and_escaped() {
    let env = posts_env().await;
    let got = search_posts(&env.pool, "котик", 10).await.expect("search_posts");
    let snippet = |id: i64| got.iter().find(|h| h.id == id).map(|h| h.snippet.clone()).unwrap_or_default();
    assert!(snippet(2).contains("<mark>котик</mark>"), "в выдержке поста 2 слово выделено: {:?}", snippet(2));
    assert!(snippet(3).contains("<mark>котики</mark>"), "выделяется форма из текста: {:?}", snippet(3));
    let long = snippet(5);
    assert!(long.contains("<mark>котик</mark>") && long.split_whitespace().count() <= 14, "выдержка из длинного текста короткая и с выделением: {long:?}");
    let risky = snippet(7);
    let plain = risky.replace("<mark>", "").replace("</mark>", "");
    assert!(!plain.contains('<'), "кроме <mark> в выдержке не должно быть символа «<», а в посте 7 был оборванный тег: {risky:?}");
    assert!(!plain.contains("& ") , "символ & должен стать сущностью: {risky:?}");
}

#[tokio::test]
async fn blank_queries_are_not_errors() {
    let env = posts_env().await;
    assert!(search_posts(&env.pool, "", 10).await.expect("пустой запрос не ошибка").is_empty(), "пустой запрос: пустой список");
    assert!(search_posts(&env.pool, "   ", 10).await.expect("пробелы не ошибка").is_empty(), "пробелы: пустой список");
    assert!(search_posts(&env.pool, "-", 10).await.expect("один минус не ошибка").is_empty(), "один минус: пустой список");
}

#[tokio::test]
async fn people_with_typos() {
    let env = Env::new().await;
    env.run(
        "INSERT INTO users (username) SELECT substr(md5(g::text), 1, 8) FROM generate_series(1, 2000) g;
         INSERT INTO users (username) VALUES ('alice'), ('alina'), ('elise'), ('alise2'), ('bob'), ('maria')",
    )
    .await;
    let got = find_people(&env.pool, "alise").await.expect("find_people");
    assert_eq!(got, vec!["alise2", "alice", "alina", "elise"], "сначала самый похожий, потом равные по алфавиту; bob и maria не похожи");
    let got = find_people(&env.pool, "ALISE").await.expect("find_people");
    assert_eq!(got.first().map(String::as_str), Some("alise2"), "регистр не важен");
    assert!(find_people(&env.pool, "qqqqqq").await.expect("find_people").is_empty(), "ничего похожего: пусто");
}

#[tokio::test]
async fn a_rare_word_is_found_through_the_index() {
    let env = Env::new().await;
    env.run(
        "INSERT INTO posts SELECT g, 'Заметка ' || g, 'обычный текст про погоду и кофе номер ' || g || CASE WHEN g % 2000 = 0 THEN ' клавикорд' ELSE '' END FROM generate_series(1, 30000) g;
         ANALYZE posts",
    )
    .await;
    let plan = env.rows("EXPLAIN SELECT id FROM posts WHERE search @@ websearch_to_tsquery('russian', 'клавикорд')").await.join("\n");
    assert!(plan.contains("Bitmap Index Scan") && !plan.contains("Seq Scan on posts"), "редкое слово должно искаться по индексу, а план такой:\n{plan}");
    let got = search_posts(&env.pool, "клавикорд", 100).await.expect("search_posts");
    assert_eq!(got.len(), 15, "слово есть у каждого двухтысячного поста из 30000");
}
