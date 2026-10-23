fn social_schema() -> &'static str {
    "CREATE TABLE users (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        username text NOT NULL UNIQUE CHECK (length(username) BETWEEN 3 AND 30),
        email text NOT NULL UNIQUE,
        bio text,
        created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE posts (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        author_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        body text NOT NULL CHECK (length(trim(body)) > 0),
        likes_count integer NOT NULL DEFAULT 0 CHECK (likes_count >= 0),
        created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE follows (
        follower_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        followee_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY (follower_id, followee_id),
        CHECK (follower_id <> followee_id)
    );
    CREATE TABLE likes (
        user_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY (user_id, post_id)
    );
    CREATE TABLE comments (
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        author_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        parent_id bigint REFERENCES comments (id) ON DELETE SET NULL,
        body text NOT NULL CHECK (length(trim(body)) > 0),
        created_at timestamptz NOT NULL DEFAULT now()
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
        db.run(social_schema()).await;
        db.run(
            "INSERT INTO users (username, email) VALUES ('alice', 'a@x.org'), ('bob', 'b@x.org'), ('carol', 'c@x.org');
             INSERT INTO posts (author_id, body) VALUES (1, 'first'), (2, 'second');
             INSERT INTO comments (post_id, author_id, parent_id, body) VALUES (1, 2, NULL, 'root'), (1, 3, 1, 'reply')",
        )
        .await;
        db
    }

    #[tokio::test]
    async fn users_rules() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO users (id, username, email) VALUES (50, 'dave', 'd@x.org')", "428C9", "ручной id").await;
        expect_error(&db, "INSERT INTO users (username, email) VALUES ('alice', 'new@x.org')", "23505", "повторный ник").await;
        expect_error(&db, "INSERT INTO users (username, email) VALUES ('dave', 'a@x.org')", "23505", "повторная почта").await;
        expect_error(&db, "INSERT INTO users (username, email) VALUES ('ab', 'd@x.org')", "23514", "короткий ник").await;
        expect_error(&db, "INSERT INTO users (username, email) VALUES (repeat('x', 31), 'd@x.org')", "23514", "длинный ник").await;
        expect_error(&db, "INSERT INTO users (username, email) VALUES ('dave', NULL)", "23502", "почта обязательна").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM users WHERE created_at > now() - interval '1 minute'").await, "3", "created_at по умолчанию «сейчас»");
        assert_eq!(cell(&db, "SELECT bio IS NULL FROM users WHERE id = 1").await, "true", "bio необязательна");
    }

    #[tokio::test]
    async fn posts_rules() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO posts (author_id, body) VALUES (99, 'x')", "23503", "несуществующий автор").await;
        expect_error(&db, "INSERT INTO posts (author_id, body) VALUES (1, '  ')", "23514", "пустой текст").await;
        expect_error(&db, "INSERT INTO posts (author_id, body, likes_count) VALUES (1, 'x', -1)", "23514", "отрицательные лайки").await;
        expect_error(&db, "INSERT INTO posts (author_id, body, likes_count) VALUES (1, 'x', NULL)", "23502", "лайки обязательны").await;
        assert_eq!(cell(&db, "SELECT likes_count FROM posts WHERE id = 1").await, "0", "лайки по умолчанию 0");
    }

    #[tokio::test]
    async fn deleting_author_removes_posts_and_their_comments() {
        let db = prepared().await;
        db.run("DELETE FROM users WHERE id = 1").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM posts WHERE author_id = 1").await, "0", "посты удалённого автора должны исчезнуть");
        assert_eq!(cell(&db, "SELECT count(*) FROM comments").await, "0", "комментарии под этим постом тоже");
    }

    #[tokio::test]
    async fn follows_rules() {
        let db = prepared().await;
        db.run("INSERT INTO follows (follower_id, followee_id) VALUES (1, 2), (2, 1)").await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (1, 2)", "23505", "повторная подписка").await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (3, 3)", "23514", "подписка на себя").await;
        expect_error(&db, "INSERT INTO follows (follower_id, followee_id) VALUES (3, 99)", "23503", "несуществующий пользователь").await;
        db.run("DELETE FROM users WHERE id = 2").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM follows").await, "0", "подписки удалённого пользователя исчезают в обе стороны");
    }

    #[tokio::test]
    async fn likes_rules() {
        let db = prepared().await;
        db.run("INSERT INTO likes (user_id, post_id) VALUES (1, 1), (2, 1), (1, 2)").await;
        expect_error(&db, "INSERT INTO likes (user_id, post_id) VALUES (1, 1)", "23505", "повторный лайк").await;
        expect_error(&db, "INSERT INTO likes (user_id, post_id) VALUES (1, 99)", "23503", "лайк несуществующего поста").await;
        db.run("DELETE FROM posts WHERE id = 1").await;
        assert_eq!(cell(&db, "SELECT count(*) FROM likes").await, "1", "лайки удалённого поста исчезают");
    }

    #[tokio::test]
    async fn comments_rules() {
        let db = prepared().await;
        expect_error(&db, "INSERT INTO comments (post_id, author_id, body) VALUES (1, 1, ' ')", "23514", "пустой комментарий").await;
        expect_error(&db, "INSERT INTO comments (post_id, author_id, parent_id, body) VALUES (1, 1, 99, 'x')", "23503", "несуществующий родитель").await;
        db.run("DELETE FROM comments WHERE id = 1").await;
        let t = db.table("SELECT id, parent_id FROM comments").await;
        assert_eq!(t.rows, [["2", "NULL"]], "ответ должен пережить удаление родителя:\n{t}");
    }
}
