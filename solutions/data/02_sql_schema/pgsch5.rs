fn posts_table() -> &'static str {
    "CREATE TABLE posts (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        body text NOT NULL,
        likes integer NOT NULL DEFAULT 0,
        created_at timestamptz NOT NULL DEFAULT now(),
        is_draft boolean NOT NULL DEFAULT true
    )"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;


    async fn expect_error(db: &TestDb, sql: &str, code: &str, why: &str) {
        let err = db
            .fails(sql)
            .await
            .unwrap_or_else(|| panic!("{why}: Postgres принял команду, а должен был отвергнуть:\n{sql}"));
        assert_eq!(err.code, code, "{why}: ошибка не та, что ждали (SQLSTATE {code}). База сказала: {}", err.message);
    }

    #[tokio::test]
    async fn defaults_fill_the_row() {
        let db = TestDb::new().await;
        db.run(posts_table()).await;
        db.run("INSERT INTO posts (body) VALUES ('hello')").await;
        let t = db.table("SELECT likes, is_draft, created_at > now() - interval '1 minute' FROM posts").await;
        assert_eq!(t.rows[0], ["0", "true", "true"], "лайки 0, черновик true, время создания «сейчас»:\n{t}");
    }

    #[tokio::test]
    async fn body_is_required() {
        let db = TestDb::new().await;
        db.run(posts_table()).await;
        expect_error(&db, "INSERT INTO posts DEFAULT VALUES", "23502", "пост без текста").await;
    }

    #[tokio::test]
    async fn explicit_null_is_rejected() {
        let db = TestDb::new().await;
        db.run(posts_table()).await;
        expect_error(&db, "INSERT INTO posts (body, likes) VALUES ('x', NULL)", "23502", "явный NULL в лайках").await;
        expect_error(&db, "INSERT INTO posts (body, created_at) VALUES ('x', NULL)", "23502", "явный NULL во времени").await;
        expect_error(&db, "INSERT INTO posts (body, is_draft) VALUES ('x', NULL)", "23502", "явный NULL в черновике").await;
    }
}
