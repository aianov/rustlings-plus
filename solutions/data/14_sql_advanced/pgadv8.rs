fn search_stemmed() -> &'static str {
    "SELECT id FROM posts WHERE to_tsvector('russian', body) @@ websearch_to_tsquery('russian', $1) ORDER BY id"
}

fn search_exact() -> &'static str {
    "SELECT id FROM posts WHERE to_tsvector('simple', body) @@ websearch_to_tsquery('simple', $1) ORDER BY id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "
        CREATE TABLE posts (id bigint PRIMARY KEY, body text NOT NULL);
        INSERT INTO posts VALUES
            (1, 'Мой котик любит бегать по крышам'),
            (2, 'Два котика и одна собака во дворе'),
            (3, 'Собака гуляет, а котики спят'),
            (4, 'Rust 2024 вышел: что нового в языке'),
            (5, 'Учим Rust по вечерам, собака мешает'),
            (6, 'Новый кот в доме');
    ";

    async fn ids(db: &TestDb, sql: &str, q: &str) -> Vec<i64> {
        sqlx::query_scalar(sql).bind(q).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("запрос упал на {q:?}: {e}"))
    }

    #[tokio::test]
    async fn morphology_finds_every_form() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(ids(&db, search_stemmed(), "котик").await, vec![1, 2, 3], "слово «котик» должно находить «котика» и «котики», но не «кот»");
        assert_eq!(ids(&db, search_stemmed(), "котики").await, vec![1, 2, 3], "форма запроса не важна, важна основа");
    }

    #[tokio::test]
    async fn web_syntax_works() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(ids(&db, search_stemmed(), "котики -собака").await, vec![1], "минус исключает посты со словом «собака»");
        assert_eq!(ids(&db, search_stemmed(), "собака or rust").await, vec![2, 3, 4, 5], "or даёт объединение");
        assert_eq!(ids(&db, search_stemmed(), "\"котик любит\"").await, vec![1], "кавычки ищут слова подряд");
    }

    #[tokio::test]
    async fn exact_search_has_no_morphology() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(ids(&db, search_exact(), "котики").await, vec![3], "точный поиск находит только форму «котики»");
        assert_eq!(ids(&db, search_exact(), "rust -собака").await, vec![4], "синтаксис запроса работает и без морфологии");
    }
}
