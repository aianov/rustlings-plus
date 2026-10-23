// Лента пользователя, главный запрос соцсети. $1 это id того, кто смотрит ленту.
// Нужны посты тех, на кого он подписан (follows), но не авторов, которых он заблокировал (blocks:
// blocker_id блокирует blocked_id). К каждому посту: его id, ник автора, текст и флаг liked_by_me,
// true, если смотрящий этот пост лайкнул. Один пост это одна строка, как бы много людей его ни
// лайкнули. Порядок: сначала новые посты (created_at), при равенстве больший id.
// Столбцы: id, username, body, liked_by_me. Чужие блокировки и чужие лайки на результат не влияют.
// TODO: Напиши запрос.

fn feed() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn feed_of(db: &TestDb, viewer: i64) -> Vec<(i64, String, String, bool)> {
        sqlx::query_as::<_, (i64, String, String, bool)>(feed())
            .bind(viewer)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("лента для пользователя {viewer}: {e}"))
    }

    fn row(id: i64, user: &str, body: &str, liked: bool) -> (i64, String, String, bool) {
        (id, user.to_string(), body.to_string(), liked)
    }

    #[tokio::test]
    async fn alice_sees_followees_without_blocked() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            feed_of(&db, 1).await,
            [row(4, "carol", "photos", false), row(2, "bob", "coffee", true), row(5, "bob", "monday", true), row(7, "carol", "sunset", false)],
            "alice подписана на bob, carol и dave; dave она заблокировала; лайкнула посты 2 и 5"
        );
    }

    #[tokio::test]
    async fn many_likers_do_not_multiply_rows() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            feed_of(&db, 3).await,
            [row(6, "dave", "hello", false), row(3, "alice", "async", false), row(1, "alice", "rust tips", true)],
            "у поста 1 два лайка (bob и carol), но в ленте carol он один раз и с флагом true"
        );
    }

    #[tokio::test]
    async fn only_my_blocks_count() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            feed_of(&db, 2).await,
            [row(3, "alice", "async", true), row(1, "alice", "rust tips", true)],
            "alice заблокирована frank, но это не мешает bob читать её посты"
        );
        assert!(feed_of(&db, 6).await.is_empty(), "frank заблокировал единственную, на кого подписан: лента пуста");
        assert!(feed_of(&db, 7).await.is_empty(), "grace ни на кого не подписана: лента пуста");
    }
}
