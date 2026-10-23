use sqlx::PgPool;

pub const SCHEMA: &str = include_str!("schema.sql");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostHit {
    pub id: i64,
    pub snippet: String,
}

pub async fn search_posts(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<PostHit>, sqlx::Error> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT p.id,
                ts_headline('russian',
                            replace(replace(replace(p.body, '&', '&amp;'), '<', '&lt;'), '>', '&gt;'),
                            q.query,
                            'StartSel=<mark>, StopSel=</mark>, MaxWords=12, MinWords=5')
         FROM posts p, websearch_to_tsquery('russian', $1) AS q(query)
         WHERE p.search @@ q.query
         ORDER BY ts_rank(p.search, q.query) DESC, p.id
         LIMIT $2",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id, snippet)| PostHit { id, snippet }).collect())
}

pub async fn find_people(pool: &PgPool, nick: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT username FROM users WHERE username % $1 ORDER BY similarity(username, $1) DESC, username LIMIT 5")
        .bind(nick)
        .fetch_all(pool)
        .await
}
