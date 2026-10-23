fn follows_table() -> &'static str {
    "CREATE TABLE users (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        username text NOT NULL
    );
    CREATE TABLE follows (
        follower_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        followee_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY (follower_id, followee_id),
        CHECK (follower_id <> followee_id)
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

    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run(follows_table()).await;
        db.run("INSERT INTO users (username) VALUES ('a'), ('b'), ('c')").await;
        db
    }

    #[tokio::test]
    async fn basic_follow_and_default_time() {
        let db = prepared().await;
        db.run("INSERT INTO follows (follower_id, followee_id) VALUES (1, 2), (2, 1), (1, 3)").await;
        assert_eq!(
            cell(&db, "SELECT count(*) FROM follows WHERE created_at > now() - interval '1 minute'").await,
            "3",
            "время подписки должно ставиться само"
        );
    }

    #[tokio::test]
    async fn duplicate_follow_is_rejected() {
        let db = prepared().await;
        db.run("INSERT INTO follows (follower_id, followee_id) VALUES (1, 2)").await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (1, 2)", "23505", "повторная подписка").await;
    }

    #[tokio::test]
    async fn cannot_follow_yourself() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (1, 1)", "23514", "подписка на себя").await;
    }

    #[tokio::test]
    async fn both_sides_must_exist() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (1, 99)", "23503", "несуществующий автор").await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (99, 1)", "23503", "несуществующий подписчик").await;
    }

    #[tokio::test]
    async fn deleting_user_removes_both_directions() {
        let db = prepared().await;
        db.run("INSERT INTO follows (follower_id, followee_id) VALUES (1, 2), (2, 3), (3, 1)").await;
        db.run("DELETE FROM users WHERE id = 2").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM follows").await, "1", "должна остаться только подписка 3 на 1");
    }
}
