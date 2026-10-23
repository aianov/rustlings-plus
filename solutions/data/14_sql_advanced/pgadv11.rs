fn similar_nicks() -> &'static str {
    "SELECT username FROM users WHERE username % $1 ORDER BY similarity(username, $1) DESC, username LIMIT 3"
}

fn substring_index() -> &'static str {
    "CREATE INDEX users_username_trgm ON users USING gin (username gin_trgm_ops)"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn ensure_trgm(db: &TestDb) {
        let mut conn = db.pool.acquire().await.expect("соединение");
        sqlx::raw_sql("SELECT pg_advisory_lock(14011)").execute(&mut *conn).await.expect("замок");
        let res = sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA public").execute(&mut *conn).await;
        sqlx::raw_sql("SELECT pg_advisory_unlock(14011)").execute(&mut *conn).await.expect("замок");
        res.expect("расширение pg_trgm должно ставиться (оно входит в состав Postgres)");
    }

    async fn plan_text(db: &TestDb, sql: &str) -> String {
        db.table(&format!("EXPLAIN {sql}")).await.column(0).join("\n")
    }

    const SEED: &str = "
        CREATE TABLE users (id bigserial PRIMARY KEY, username text NOT NULL);
        INSERT INTO users (username) SELECT substr(md5(g::text), 1, 8) FROM generate_series(1, 30000) g;
        INSERT INTO users (username) VALUES ('alice'), ('alina'), ('elise'), ('alise2'), ('bob'), ('bobby'), ('maria'), ('marya'), ('zq7_rare');
        ANALYZE users;
    ";

    #[tokio::test]
    async fn typos_find_the_right_person() {
        let db = TestDb::with_sql(SEED).await;
        ensure_trgm(&db).await;
        let names: Vec<String> = sqlx::query_scalar(similar_nicks()).bind("alise").fetch_all(&db.pool).await.expect("similar_nicks");
        assert_eq!(names, vec!["alise2", "alice", "alina"], "самый похожий alise2, потом alice и alina (равны по похожести, по алфавиту), elise не влезает в три, bob не похож совсем");
        let names: Vec<String> = sqlx::query_scalar(similar_nicks()).bind("marja").fetch_all(&db.pool).await.expect("similar_nicks");
        assert_eq!(names, vec!["maria", "marya"], "для «marja» подходят только maria и marya");
    }

    #[tokio::test]
    async fn nothing_similar_gives_nothing() {
        let db = TestDb::with_sql(SEED).await;
        ensure_trgm(&db).await;
        let names: Vec<String> = sqlx::query_scalar(similar_nicks()).bind("qqqqqq").fetch_all(&db.pool).await.expect("similar_nicks");
        assert!(names.is_empty(), "ничего похожего на «qqqqqq» нет, а вернулось {names:?}: берём только то, что прошло порог похожести");
    }

    #[tokio::test]
    async fn substring_search_uses_the_index() {
        let db = TestDb::with_sql(SEED).await;
        ensure_trgm(&db).await;
        let query = "SELECT username FROM users WHERE username ILIKE '%zq7%'";
        let before = plan_text(&db, query).await;
        assert!(before.contains("Seq Scan on users"), "без индекса ожидался полный перебор:\n{before}");
        db.run(substring_index()).await;
        db.run("ANALYZE users").await;
        let plan = plan_text(&db, query).await;
        assert!(plan.contains("Bitmap Index Scan") && !plan.contains("Seq Scan on users"), "ILIKE с процентом с обеих сторон должен использовать триграммный индекс, а план такой:\n{plan}");
        assert_eq!(db.table(query).await.column(0), vec!["zq7_rare"], "результат не изменился");
    }
}
