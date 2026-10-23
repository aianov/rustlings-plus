fn schema() -> &'static str {
    "CREATE TABLE users (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         username text NOT NULL UNIQUE
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
    async fn cell(db: &TestDb, sql: &str) -> String {
        db.table(sql).await.cell(0, 0).to_string()
    }

    #[tokio::test]
    async fn renaming_keeps_posts_attached() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('alice'), ('bob'); INSERT INTO posts (author_id, body) VALUES (1, 'hi')").await;
        db.run("UPDATE users SET username = 'alice2' WHERE id = 1").await;
        assert_eq!(
            cell(&db, "SELECT u.username FROM posts p JOIN users u ON u.id = p.author_id WHERE p.body = 'hi'").await,
            "alice2",
            "после смены ника пост остаётся за тем же человеком"
        );
    }

    #[tokio::test]
    async fn usernames_stay_unique() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('alice'), ('bob')").await;
        expect_error(&db, "INSERT INTO users (username) VALUES ('alice')", "23505", "повторный ник").await;
        expect_error(&db, "UPDATE users SET username = 'bob' WHERE id = 1", "23505", "переименование в занятый ник").await;
        expect_error(&db, "INSERT INTO users (username) VALUES (NULL)", "23502", "ник обязателен").await;
    }
}
