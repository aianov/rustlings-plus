fn services_schema(p: &str) -> String {
    format!(
        "CREATE SCHEMA {p}_accounts;
         CREATE SCHEMA {p}_profiles;
         CREATE SCHEMA {p}_posts;
         CREATE SCHEMA {p}_social;
         CREATE TABLE {p}_accounts.accounts (
             id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             email text NOT NULL,
             password_hash text NOT NULL,
             created_at timestamptz NOT NULL DEFAULT now()
         );
         CREATE UNIQUE INDEX accounts_email_lower ON {p}_accounts.accounts (lower(email));
         CREATE TABLE {p}_profiles.profiles (
             account_id bigint PRIMARY KEY,
             username text NOT NULL CHECK (char_length(username) BETWEEN 3 AND 30),
             display_name text,
             bio text
         );
         CREATE UNIQUE INDEX profiles_username_lower ON {p}_profiles.profiles (lower(username));
         CREATE TABLE {p}_posts.posts (
             id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             author_id bigint NOT NULL,
             body text NOT NULL CHECK (btrim(body) <> ''),
             status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published')),
             likes_count integer NOT NULL DEFAULT 0 CHECK (likes_count >= 0),
             created_at timestamptz NOT NULL DEFAULT now(),
             deleted_at timestamptz
         );
         CREATE TABLE {p}_posts.comments (
             id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             post_id bigint NOT NULL REFERENCES {p}_posts.posts (id) ON DELETE CASCADE,
             author_id bigint NOT NULL,
             parent_id bigint REFERENCES {p}_posts.comments (id) ON DELETE SET NULL,
             body text NOT NULL CHECK (btrim(body) <> ''),
             created_at timestamptz NOT NULL DEFAULT now()
         );
         CREATE TABLE {p}_posts.tags (
             id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             name text NOT NULL UNIQUE
         );
         CREATE TABLE {p}_posts.post_tags (
             post_id bigint NOT NULL REFERENCES {p}_posts.posts (id) ON DELETE CASCADE,
             tag_id bigint NOT NULL REFERENCES {p}_posts.tags (id) ON DELETE CASCADE,
             PRIMARY KEY (post_id, tag_id)
         );
         CREATE TABLE {p}_social.likes (
             user_id bigint NOT NULL,
             post_id bigint NOT NULL,
             PRIMARY KEY (user_id, post_id)
         );
         CREATE TABLE {p}_social.follows (
             follower_id bigint NOT NULL,
             followee_id bigint NOT NULL,
             PRIMARY KEY (follower_id, followee_id),
             CHECK (follower_id <> followee_id)
         );
         CREATE TABLE {p}_social.blocks (
             blocker_id bigint NOT NULL,
             blocked_id bigint NOT NULL,
             PRIMARY KEY (blocker_id, blocked_id),
             CHECK (blocker_id <> blocked_id)
         )"
    )
}

fn feed(p: &str) -> String {
    format!(
        "SELECT po.id, po.body
         FROM {p}_posts.posts po
         JOIN {p}_social.follows f ON f.followee_id = po.author_id AND f.follower_id = $1
         WHERE po.status = 'published' AND po.deleted_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM {p}_social.blocks b WHERE b.blocker_id = $1 AND b.blocked_id = po.author_id)
         ORDER BY po.created_at DESC, po.id DESC"
    )
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

    async fn prepared() -> (TestDb, String) {
        let mut db = TestDb::new().await;
        let p = db.schema().to_string();
        for s in ["accounts", "profiles", "posts", "social"] {
            db.track_schema(&format!("{p}_{s}"));
        }
        db.run(&services_schema(&p)).await;
        (db, p)
    }

    fn q(p: &str, sql: &str) -> String {
        sql.replace('@', &format!("{p}_"))
    }

    #[tokio::test]
    async fn services_are_isolated() {
        let (db, p) = prepared().await;
        let cross = cell(&db, &format!(
            "SELECT count(*) FROM pg_constraint c JOIN pg_class a ON a.oid = c.conrelid JOIN pg_class b ON b.oid = c.confrelid
             JOIN pg_namespace na ON na.oid = a.relnamespace
             WHERE c.contype = 'f' AND a.relnamespace <> b.relnamespace AND na.nspname LIKE '{p}\\_%'"
        )).await;
        assert_eq!(cross, "0", "между схемами сервисов не должно быть внешних ключей");
        let inner = cell(&db, &format!(
            "SELECT count(*) FROM pg_constraint c JOIN pg_namespace n ON n.oid = c.connamespace WHERE c.contype = 'f' AND n.nspname = '{p}_posts'"
        )).await;
        assert_eq!(inner, "4", "внутри сервиса постов должны быть внешние ключи: comments.post_id, comments.parent_id, post_tags.post_id, post_tags.tag_id");
    }

    #[tokio::test]
    async fn accounts_and_profiles() {
        let (db, p) = prepared().await;
        db.run(&q(&p, "INSERT INTO @accounts.accounts (email, password_hash) VALUES ('A@x.org', 'h')")).await;
        expect_error(&db, &q(&p, "INSERT INTO @accounts.accounts (email, password_hash) VALUES ('a@X.org', 'h')"), "23505", "почта без учёта регистра").await;
        expect_error(&db, &q(&p, "INSERT INTO @accounts.accounts (email, password_hash) VALUES ('b@x.org', NULL)"), "23502", "хеш обязателен").await;
        assert_eq!(cell(&db, &q(&p, "SELECT count(*) FROM @accounts.accounts WHERE created_at > now() - interval '1 minute'")).await, "1", "created_at по умолчанию «сейчас»");
        db.run(&q(&p, "INSERT INTO @profiles.profiles (account_id, username) VALUES (500, 'Alice')")).await;
        expect_error(&db, &q(&p, "INSERT INTO @profiles.profiles (account_id, username) VALUES (501, 'alice')"), "23505", "ник без учёта регистра").await;
        expect_error(&db, &q(&p, "INSERT INTO @profiles.profiles (account_id, username) VALUES (502, 'ab')"), "23514", "короткий ник").await;
        expect_error(&db, &q(&p, "INSERT INTO @profiles.profiles (account_id, username) VALUES (500, 'second')"), "23505", "второй профиль того же аккаунта").await;
        assert_eq!(cell(&db, &q(&p, "SELECT username FROM @profiles.profiles")).await, "Alice", "ник хранится как введён");
    }

    #[tokio::test]
    async fn posts_service_rules() {
        let (db, p) = prepared().await;
        db.run(&q(&p, "INSERT INTO @posts.posts (author_id, body) VALUES (1, 'hello'); INSERT INTO @posts.comments (post_id, author_id, parent_id, body) VALUES (1, 2, NULL, 'root'), (1, 3, 1, 'reply')")).await;
        assert_eq!(cell(&db, &q(&p, "SELECT status || '/' || likes_count FROM @posts.posts")).await, "draft/0", "по умолчанию draft и 0 лайков");
        expect_error(&db, &q(&p, "INSERT INTO @posts.posts (author_id, body) VALUES (1, '   ')"), "23514", "пустой текст").await;
        expect_error(&db, &q(&p, "INSERT INTO @posts.posts (author_id, body, status) VALUES (1, 'x', 'banned')"), "23514", "неизвестный статус").await;
        expect_error(&db, &q(&p, "INSERT INTO @posts.posts (author_id, body, likes_count) VALUES (1, 'x', -1)"), "23514", "отрицательный счётчик").await;
        expect_error(&db, &q(&p, "INSERT INTO @posts.posts (author_id, body) VALUES (NULL, 'x')"), "23502", "автор обязателен").await;
        expect_error(&db, &q(&p, "INSERT INTO @posts.comments (post_id, author_id, body) VALUES (99, 1, 'x')"), "23503", "комментарий к несуществующему посту").await;
        db.run(&q(&p, "DELETE FROM @posts.comments WHERE id = 1")).await;
        assert_eq!(cell(&db, &q(&p, "SELECT parent_id IS NULL FROM @posts.comments WHERE id = 2")).await, "true", "ответ остаётся, parent_id становится NULL");
        db.run(&q(&p, "DELETE FROM @posts.posts WHERE id = 1")).await;
        assert_eq!(cell(&db, &q(&p, "SELECT count(*) FROM @posts.comments")).await, "0", "комментарии удаляются вместе с постом");
    }

    #[tokio::test]
    async fn tags_and_social_rules() {
        let (db, p) = prepared().await;
        db.run(&q(&p, "INSERT INTO @posts.posts (author_id, body) VALUES (1, 'a'); INSERT INTO @posts.tags (name) VALUES ('rust'); INSERT INTO @posts.post_tags VALUES (1, 1)")).await;
        expect_error(&db, &q(&p, "INSERT INTO @posts.tags (name) VALUES ('rust')"), "23505", "повторный тег").await;
        expect_error(&db, &q(&p, "INSERT INTO @posts.post_tags VALUES (1, 1)"), "23505", "повторная связь").await;
        db.run(&q(&p, "DELETE FROM @posts.tags")).await;
        assert_eq!(cell(&db, &q(&p, "SELECT count(*) FROM @posts.post_tags")).await, "0", "связи удалённого тега исчезают");
        db.run(&q(&p, "INSERT INTO @social.likes VALUES (7, 4242), (8, 4242); INSERT INTO @social.follows VALUES (1, 2); INSERT INTO @social.blocks VALUES (1, 3)")).await;
        expect_error(&db, &q(&p, "INSERT INTO @social.likes VALUES (7, 4242)"), "23505", "повторный лайк").await;
        expect_error(&db, &q(&p, "INSERT INTO @social.follows VALUES (1, 2)"), "23505", "повторная подписка").await;
        expect_error(&db, &q(&p, "INSERT INTO @social.follows VALUES (5, 5)"), "23514", "подписка на себя").await;
        expect_error(&db, &q(&p, "INSERT INTO @social.blocks VALUES (5, 5)"), "23514", "блокировка себя").await;
        expect_error(&db, &q(&p, "INSERT INTO @social.blocks VALUES (5, NULL)"), "23502", "оба конца обязательны").await;
    }

    #[tokio::test]
    async fn feed_query() {
        let (db, p) = prepared().await;
        db.run(&q(&p, "INSERT INTO @posts.posts (author_id, body, status, created_at, deleted_at) VALUES
                (2, 'draft',     'draft',     '2026-10-01 10:00:00+00', NULL),
                (2, 'visible-a', 'published', '2026-10-02 10:00:00+00', NULL),
                (2, 'removed',   'published', '2026-10-03 10:00:00+00', '2026-10-04 00:00:00+00'),
                (3, 'blocked',   'published', '2026-10-05 10:00:00+00', NULL),
                (4, 'visible-b', 'published', '2026-10-06 10:00:00+00', NULL),
                (5, 'unfollowed','published', '2026-10-07 10:00:00+00', NULL);
              INSERT INTO @social.follows VALUES (1, 2), (1, 3), (1, 4), (9, 5);
              INSERT INTO @social.blocks VALUES (1, 3), (9, 4)")).await;
        let rows = sqlx::query_as::<_, (i64, String)>(&feed(&p)).bind(1_i64).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("feed: {e}"));
        let bodies: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(bodies, ["visible-b", "visible-a"], "в ленте alice: опубликованные неудалённые посты тех, на кого она подписана, без заблокированного автора 3, от новых к старым");
        let rows = sqlx::query_as::<_, (i64, String)>(&feed(&p)).bind(9_i64).fetch_all(&db.pool).await.unwrap_or_else(|e| panic!("feed: {e}"));
        assert_eq!(rows.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(), ["unfollowed"], "лента пользователя 9: блокировка автора 4 не мешает постам автора 5, а чужие блокировки не действуют");
    }
}
