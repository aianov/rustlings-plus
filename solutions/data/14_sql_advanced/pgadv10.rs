fn ranked() -> &'static str {
    "SELECT p.id, ts_headline('russian', p.body, q.query, 'StartSel=<mark>, StopSel=</mark>, MaxWords=12, MinWords=5') AS snippet
     FROM posts p, websearch_to_tsquery('russian', $1) AS q(query)
     WHERE p.search @@ q.query
     ORDER BY ts_rank(p.search, q.query) DESC, p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "
        CREATE TABLE posts (
            id bigint PRIMARY KEY, title text NOT NULL, body text NOT NULL,
            search tsvector GENERATED ALWAYS AS (setweight(to_tsvector('russian', title), 'A') || setweight(to_tsvector('russian', body), 'B')) STORED
        );
        CREATE INDEX posts_search_idx ON posts USING gin (search);
        INSERT INTO posts (id, title, body) VALUES
            (1, 'Котик', 'просто текст про погоду'),
            (2, 'Погода', 'котик гуляет, котик спит, котик ест'),
            (3, 'Про собак', 'проходили котики мимо'),
            (4, 'Без темы', 'ни о чём'),
            (5, 'Длинная история', repeat('слово ', 30) || 'котик ' || repeat('слово ', 30));
    ";

    async fn run(db: &TestDb, q: &str) -> Vec<(i64, String)> {
        let sql = format!("SELECT r.id::bigint, r.snippet::text FROM ({}) AS r", ranked());
        sqlx::query_as(&sql).bind(q).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос ranked упал: {e}"))
    }

    #[tokio::test]
    async fn weights_decide_the_order() {
        let db = TestDb::with_sql(SEED).await;
        let got = run(&db, "котик").await;
        let ids: Vec<i64> = got.iter().map(|r| r.0).collect();
        assert_eq!(ids, vec![1, 2, 3, 5], "пост 1 (слово в заголовке) выше поста 2 (три раза в тексте), потом одно упоминание в тексте, при равенстве по id; пост 4 не подходит");
    }

    #[tokio::test]
    async fn snippets_highlight_matches() {
        let db = TestDb::with_sql(SEED).await;
        let got = run(&db, "котик").await;
        let by_id = |id: i64| got.iter().find(|r| r.0 == id).map(|r| r.1.clone()).unwrap_or_default();
        assert!(by_id(2).contains("<mark>котик</mark>"), "в выдержке поста 2 слово должно быть выделено: {:?}", by_id(2));
        assert!(by_id(3).contains("<mark>котики</mark>"), "выделяется форма слова из текста, а не из запроса: {:?}", by_id(3));
        let long = by_id(5);
        assert!(long.contains("<mark>котик</mark>"), "в длинном тексте выдержка должна вокруг найденного слова: {long:?}");
        assert!(long.split_whitespace().count() <= 14, "выдержка из поста 5 должна быть короткой (до 12 слов), а в ней {} слов: {long:?}", long.split_whitespace().count());
    }
}
