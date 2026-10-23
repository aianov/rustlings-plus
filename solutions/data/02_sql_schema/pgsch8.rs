fn authors_and_posts() -> &'static str {
    "CREATE TABLE users (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        username text NOT NULL
    );
    CREATE TABLE posts (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        author_id bigint NOT NULL REFERENCES users (id),
        body text NOT NULL
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
    async fn orphan_post_is_rejected() {
        let db = TestDb::new().await;
        db.run(authors_and_posts()).await;
        db.run("INSERT INTO users (username) VALUES ('alice')").await;
        expect_error(&db, "INSERT INTO posts (author_id, body) VALUES (999, 'x')", "23503", "пост несуществующего автора").await;
        db.run("INSERT INTO posts (author_id, body) VALUES (1, 'x')").await;
    }

    #[tokio::test]
    async fn author_with_posts_cannot_be_deleted() {
        let db = TestDb::new().await;
        db.run(authors_and_posts()).await;
        db.run("INSERT INTO users (username) VALUES ('alice'), ('bob')").await;
        db.run("INSERT INTO posts (author_id, body) VALUES (1, 'x')").await;
        expect_error(&db, "DELETE FROM users WHERE id = 1", "23503", "удаление автора с постами").await;
        db.run("DELETE FROM users WHERE id = 2").await;
    }
}
