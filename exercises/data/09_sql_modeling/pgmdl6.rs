// В схеме мягкое удаление: у постов есть deleted_at. Но удалённые посты продолжают показываться
// в ленте, а в профиле автор видит завышенные числа. Нужны два запроса.
//  - `feed`: живые посты (столбцы id, body), от новых к старым по created_at.
//  - `profile_counts` ($1 id автора): одна строка со столбцами posts (число живых постов) и likes
//    (лайки только на живых постах). У автора без постов нули.
// Чтобы условие не забывали, заведи представление live_posts (скрипт `live_view`) и пиши оба
// запроса по нему.
// TODO: Найди, чего не хватает запросам, и исправь.

fn live_view() -> &'static str {
    "SELECT 1"
}

fn feed() -> &'static str {
    "SELECT id, body FROM posts ORDER BY created_at DESC"
}

fn profile_counts() -> &'static str {
    "SELECT (SELECT count(*) FROM posts WHERE author_id = $1) AS posts,
            (SELECT count(*) FROM likes l JOIN posts p ON p.id = l.post_id WHERE p.author_id = $1) AS likes"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    async fn prepared() -> TestDb {
        let db = TestDb::new().await;
        db.run(
            "CREATE TABLE users (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, username text NOT NULL);
             CREATE TABLE posts (id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, author_id bigint NOT NULL REFERENCES users (id), body text NOT NULL,
                                 created_at timestamptz NOT NULL, deleted_at timestamptz);
             CREATE TABLE likes (post_id bigint NOT NULL REFERENCES posts (id), user_id bigint NOT NULL REFERENCES users (id), PRIMARY KEY (post_id, user_id));
             INSERT INTO users (username) VALUES ('alice'), ('bob'), ('carol'), ('dave');
             INSERT INTO posts (author_id, body, created_at, deleted_at) VALUES
                 (1, 'first',  '2026-10-01 10:00:00+00', NULL),
                 (1, 'second', '2026-10-02 10:00:00+00', '2026-10-03 00:00:00+00'),
                 (1, 'third',  '2026-10-03 10:00:00+00', NULL),
                 (2, 'gone',   '2026-10-04 10:00:00+00', '2026-10-04 12:00:00+00'),
                 (2, 'fifth',  '2026-10-05 10:00:00+00', NULL);
             INSERT INTO likes VALUES (1, 2), (1, 3), (2, 2), (2, 3), (2, 4), (3, 4), (4, 1), (5, 1)",
        )
        .await;
        db.run(live_view()).await;
        db
    }

    async fn counts(db: &TestDb, user: i64) -> (i64, i64) {
        sqlx::query_as::<_, (i64, i64)>(profile_counts())
            .bind(user)
            .fetch_one(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("profile_counts({user}): {e}"))
    }

    #[tokio::test]
    async fn feed_hides_deleted() {
        let db = prepared().await;
        let t = db.table(feed()).await;
        assert_eq!(t.columns, ["id", "body"], "нужны столбцы id и body");
        assert_eq!(t.rows, [["5", "fifth"], ["3", "third"], ["1", "first"]], "в ленте только живые посты от новых к старым, удалённых 2 и 4 быть не должно:\n{t}");
    }

    #[tokio::test]
    async fn view_shows_live_rows_only() {
        let db = prepared().await;
        let t = db.table("SELECT id FROM live_posts ORDER BY id").await;
        assert_eq!(t.rows, [["1"], ["3"], ["5"]], "представление live_posts должно содержать только живые посты:\n{t}");
    }

    #[tokio::test]
    async fn counters_ignore_deleted() {
        let db = prepared().await;
        assert_eq!(counts(&db, 1).await, (2, 3), "у alice два живых поста и три лайка на них (три лайка удалённого поста не считаются)");
        assert_eq!(counts(&db, 2).await, (1, 1), "у bob один живой пост и один лайк на нём");
        assert_eq!(counts(&db, 3).await, (0, 0), "у carol постов нет: нули, не пропавшая строка");
    }
}
