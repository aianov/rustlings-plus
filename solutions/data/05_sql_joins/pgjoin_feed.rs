fn feed() -> &'static str {
    "SELECT p.id, u.username, p.body, (l.user_id IS NOT NULL) AS liked_by_me
     FROM follows f
     JOIN posts p ON p.author_id = f.followee_id
     JOIN users u ON u.id = p.author_id
     LEFT JOIN likes l ON l.post_id = p.id AND l.user_id = $1
     LEFT JOIN blocks b ON b.blocker_id = $1 AND b.blocked_id = p.author_id
     WHERE f.follower_id = $1 AND b.blocker_id IS NULL
     ORDER BY p.created_at DESC, p.id DESC"
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
