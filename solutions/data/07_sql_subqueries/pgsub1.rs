fn feed_of() -> &'static str {
    "SELECT id, author_id, body FROM posts
     WHERE author_id IN (SELECT followee_id FROM follows WHERE follower_id = $1)
     ORDER BY id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn feed(db: &TestDb, viewer: i64) -> Vec<(i64, i64, String)> {
        sqlx::query_as::<_, (i64, i64, String)>(feed_of())
            .bind(viewer)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("feed_of({viewer}): {e}"))
    }

    #[tokio::test]
    async fn only_followed_authors() {
        let db = TestDb::with_sql(SEED).await;
        let ids: Vec<i64> = feed(&db, 1).await.into_iter().map(|r| r.0).collect();
        assert_eq!(ids, [2, 4, 5, 6, 7, 10], "alice подписана на bob, carol и dave: их посты, и ничьих больше (своих тоже нет)");
        let ids: Vec<i64> = feed(&db, 5).await.into_iter().map(|r| r.0).collect();
        assert_eq!(ids, [6], "eve подписана только на dave, у него один пост");
    }

    #[tokio::test]
    async fn other_viewers() {
        let db = TestDb::with_sql(SEED).await;
        let rows = feed(&db, 4).await;
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), [9], "dave подписан на eve, у неё один пост (id 9)");
        let rows = feed(&db, 6).await;
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 3, 8], "frank подписан на alice: её три поста");
        assert_eq!(rows[0].2, "rust tips", "третий столбец это текст поста (body)");
    }
}
