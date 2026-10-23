fn checks() -> &'static str {
    "CREATE TABLE users (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        username text NOT NULL CHECK (length(username) BETWEEN 3 AND 30)
    );
    CREATE TABLE posts (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        body text NOT NULL CHECK (length(trim(body)) > 0)
    );
    CREATE TABLE reviews (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        rating integer CHECK (rating BETWEEN 1 AND 5)
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
    async fn username_length() {
        let db = TestDb::new().await;
        db.run(checks()).await;
        expect_error(&db, "INSERT INTO users (username) VALUES ('ab')", "23514", "ник из двух символов").await;
        expect_error(&db, "INSERT INTO users (username) VALUES (repeat('x', 31))", "23514", "ник из 31 символа").await;
        db.run("INSERT INTO users (username) VALUES ('abc'), (repeat('x', 30))").await;
    }

    #[tokio::test]
    async fn blank_body_is_rejected() {
        let db = TestDb::new().await;
        db.run(checks()).await;
        expect_error(&db, "INSERT INTO posts (body) VALUES ('')", "23514", "пустой текст").await;
        expect_error(&db, "INSERT INTO posts (body) VALUES ('   ')", "23514", "текст из пробелов").await;
        db.run("INSERT INTO posts (body) VALUES ('x')").await;
    }

    #[tokio::test]
    async fn rating_range_and_missing_rating() {
        let db = TestDb::new().await;
        db.run(checks()).await;
        expect_error(&db, "INSERT INTO reviews (rating) VALUES (0)", "23514", "оценка 0").await;
        expect_error(&db, "INSERT INTO reviews (rating) VALUES (6)", "23514", "оценка 6").await;
        db.run("INSERT INTO reviews (rating) VALUES (1), (5), (NULL)").await;
    }
}
