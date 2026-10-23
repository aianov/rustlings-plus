// Схема всей соцсети, разложенная по сервисам. Функция `services_schema(p)` возвращает один
// скрипт; `p` это префикс имён схем от теста. Все таблицы пишутся с именем схемы. Между схемами
// внешних ключей нет (ссылки на чужие данные это просто столбцы), внутри схемы они есть.
//  1. `{p}_accounts`.accounts: id (bigint, generated identity), email (обязателен, уникален без
//     учёта регистра), password_hash (обязателен), created_at (по умолчанию «сейчас»).
//  2. `{p}_profiles`.profiles: account_id (bigint, первичный ключ), username (обязателен, от 3 до 30
//     символов, уникален без учёта регистра), display_name, bio (необязательны).
//  3. `{p}_posts`: posts (id identity; author_id bigint обязателен; body не пуст после обрезки
//     пробелов; status 'draft' по умолчанию или 'published'; likes_count не отрицателен, по умолчанию 0;
//     created_at по умолчанию «сейчас»; deleted_at необязателен); comments (id identity; post_id
//     ссылка на posts, удаляются вместе с постом; author_id bigint обязателен; parent_id необязательная
//     ссылка на comments, при удалении родителя становится NULL; body не пуст; created_at);
//     tags (id identity, name уникален); post_tags (post_id, tag_id, пара ключ, оба ссылаются
//     и удаляются каскадом).
//  4. `{p}_social`: likes (user_id, post_id, пара ключ), follows (follower_id, followee_id, пара ключ,
//     на себя нельзя), blocks (blocker_id, blocked_id, пара ключ, на себя нельзя). Все столбцы
//     bigint, обязательны, без внешних ключей.
// Ещё функция `feed(p)`: запрос с параметром $1 (смотрящий). Возвращает столбцы id, body постов
// тех, на кого смотрящий подписан: только опубликованные и не удалённые, авторов из его блокировок
// пропустить; порядок: created_at по убыванию, затем id по убыванию.
// TODO: Напиши скрипт и запрос.

fn services_schema(p: &str) -> String {
    todo!()
}

fn feed(p: &str) -> String {
    todo!()
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
