fn schema() -> &'static str {
    "CREATE TABLE users (
         id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
         username text NOT NULL,
         deleted_at timestamptz
     );
     CREATE UNIQUE INDEX users_live_username_key ON users (username) WHERE deleted_at IS NULL"
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
    async fn deleted_username_can_be_taken_again() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('alice')").await;
        expect_error(&db, "INSERT INTO users (username) VALUES ('alice')", "23505", "ник занят живым аккаунтом").await;
        db.run("UPDATE users SET deleted_at = now() WHERE id = 1").await;
        db.run("INSERT INTO users (username) VALUES ('alice')").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM users WHERE username = 'alice'").await, "2", "старая строка осталась, новая добавилась");
        expect_error(&db, "INSERT INTO users (username) VALUES ('alice')", "23505", "третий живой alice").await;
    }

    #[tokio::test]
    async fn restoring_collides_with_the_new_owner() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('alice')").await;
        db.run("UPDATE users SET deleted_at = now() WHERE id = 1").await;
        db.run("INSERT INTO users (username) VALUES ('alice')").await;
        expect_error(&db, "UPDATE users SET deleted_at = NULL WHERE id = 1", "23505", "воскрешение удалённого при занятом нике").await;
        db.run("INSERT INTO users (username) VALUES ('bob'); UPDATE users SET deleted_at = now() WHERE username = 'bob'; UPDATE users SET deleted_at = NULL WHERE username = 'bob'").await;
    }
}
