fn likes_schema() -> &'static str {
    "CREATE TABLE likes (
         user_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
         post_id bigint REFERENCES posts (id) ON DELETE CASCADE,
         comment_id bigint REFERENCES comments (id) ON DELETE CASCADE,
         CHECK ((post_id IS NULL) <> (comment_id IS NULL))
     );
     CREATE UNIQUE INDEX likes_post_key ON likes (user_id, post_id) WHERE post_id IS NOT NULL;
     CREATE UNIQUE INDEX likes_comment_key ON likes (user_id, comment_id) WHERE comment_id IS NOT NULL"
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

    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run(
            "CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
             CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
             CREATE TABLE comments (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
             INSERT INTO users DEFAULT VALUES; INSERT INTO users DEFAULT VALUES;
             INSERT INTO posts DEFAULT VALUES; INSERT INTO posts DEFAULT VALUES;
             INSERT INTO comments DEFAULT VALUES",
        )
        .await;
        db.run(likes_schema()).await;
        db
    }

    #[tokio::test]
    async fn exactly_one_target() {
        let db = prepared().await;
        db.run("INSERT INTO likes (user_id, post_id) VALUES (1, 1); INSERT INTO likes (user_id, comment_id) VALUES (1, 1)").await;
        expect_error(&db, "INSERT INTO likes (user_id, post_id, comment_id) VALUES (2, 1, 1)", "23514", "лайк сразу и посту, и комментарию").await;
        expect_error(&db, "INSERT INTO likes (user_id) VALUES (2)", "23514", "лайк без объекта").await;
    }

    #[tokio::test]
    async fn one_like_per_user_and_target() {
        let db = prepared().await;
        db.run("INSERT INTO likes (user_id, post_id) VALUES (1, 1)").await;
        expect_error(&db, "INSERT INTO likes (user_id, post_id) VALUES (1, 1)", "23505", "повторный лайк поста (NULL в comment_id не должен делать пары разными)").await;
        db.run("INSERT INTO likes (user_id, comment_id) VALUES (1, 1)").await;
        expect_error(&db, "INSERT INTO likes (user_id, comment_id) VALUES (1, 1)", "23505", "повторный лайк комментария").await;
        db.run("INSERT INTO likes (user_id, post_id) VALUES (2, 1), (1, 2)").await;
    }

    #[tokio::test]
    async fn references_and_cascade() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO likes (user_id, post_id) VALUES (1, 99)", "23503", "несуществующий пост").await;
        expect_error(&db, "INSERT INTO likes (user_id, comment_id) VALUES (1, 99)", "23503", "несуществующий комментарий").await;
        db.run("INSERT INTO likes (user_id, post_id) VALUES (1, 1), (2, 1); INSERT INTO likes (user_id, comment_id) VALUES (1, 1)").await;
        db.run("DELETE FROM posts WHERE id = 1").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM likes").await, "1", "лайки удалённого поста исчезают, лайк комментария остаётся");
        db.run("DELETE FROM comments WHERE id = 1").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM likes").await, "0", "лайк удалённого комментария тоже исчезает");
    }
}
