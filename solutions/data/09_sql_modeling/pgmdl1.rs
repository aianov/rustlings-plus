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

fn feed() -> &'static str {
    "SELECT p.id, u.username, p.body FROM posts p JOIN users u ON u.id = p.author_id ORDER BY p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn renamed_user_is_renamed_everywhere() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        db.run("INSERT INTO users (username) VALUES ('alice'); INSERT INTO posts (author_id, body) VALUES (1, 'one'), (1, 'two')").await;
        let t = db.table(feed()).await;
        assert_eq!(t.columns, ["id", "username", "body"], "нужны столбцы id, username, body");
        db.run("UPDATE users SET username = 'alice_new' WHERE id = 1").await;
        let t = db.table(feed()).await;
        assert_eq!(
            t.rows,
            [["1", "alice_new", "one"], ["2", "alice_new", "two"]],
            "после смены ника все посты должны показывать новый ник, а лента показывает старый:\n{t}"
        );
    }

    #[tokio::test]
    async fn name_is_stored_once() {
        let db = TestDb::new().await;
        db.run(schema()).await;
        let t = db.table("SELECT column_name FROM information_schema.columns WHERE table_schema = current_schema() AND table_name = 'posts' AND column_name LIKE '%name%'").await;
        assert!(t.is_empty(), "в posts не должно остаться столбца с копией имени:\n{t}");
    }
}
