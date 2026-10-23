use sqlx::PgPool;

pub const SCHEMA: &str = include_str!("schema.sql");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostHit {
    pub id: i64,
    pub snippet: String,
}

pub async fn search_posts(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<PostHit>, sqlx::Error> {
    let _ = (pool, query, limit);
    todo!("запрос по search с рангом и выдержкой")
}

pub async fn find_people(pool: &PgPool, nick: &str) -> Result<Vec<String>, sqlx::Error> {
    let _ = (pool, nick);
    todo!("нечёткий поиск ника через pg_trgm")
}
