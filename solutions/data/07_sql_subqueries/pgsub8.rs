fn common_follows() -> &'static str {
    "SELECT followee_id FROM follows WHERE follower_id = $1
     INTERSECT
     SELECT followee_id FROM follows WHERE follower_id = $2
     ORDER BY followee_id"
}

fn suggestions() -> &'static str {
    "(SELECT followee_id FROM follows
       WHERE follower_id IN (SELECT followee_id FROM follows WHERE follower_id = $1)
      EXCEPT
      SELECT followee_id FROM follows WHERE follower_id = $1
      EXCEPT
      SELECT id FROM users WHERE id = $1)
     ORDER BY followee_id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn common(db: &TestDb, a: i64, b: i64) -> Vec<i64> {
        sqlx::query_scalar::<_, i64>(common_follows())
            .bind(a)
            .bind(b)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("common_follows({a}, {b}): {e}"))
    }

    async fn suggest(db: &TestDb, user: i64) -> Vec<i64> {
        sqlx::query_scalar::<_, i64>(suggestions())
            .bind(user)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("suggestions({user}): {e}"))
    }

    #[tokio::test]
    async fn common_follows_are_in_both_lists() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(common(&db, 1, 2).await, [3, 4], "alice и bob оба подписаны на carol и dave");
        assert_eq!(common(&db, 1, 5).await, [4], "alice и eve сходятся только на dave");
        assert_eq!(common(&db, 6, 7).await, [1], "frank и grace оба подписаны на alice");
        assert!(common(&db, 1, 7).await.is_empty(), "alice и grace не сходятся ни в ком");
    }

    #[tokio::test]
    async fn suggestions_exclude_known_and_self() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(suggest(&db, 1).await, [5, 6], "подписки alice подписаны на eve и frank, на которых она ещё не подписана");
        assert_eq!(suggest(&db, 2).await, [5, 6], "для bob то же: себя (2) и уже знакомых (1, 3, 4) в списке быть не должно");
        assert!(suggest(&db, 5).await.is_empty(), "eve подписана на dave, а dave подписан обратно на eve: себя предлагать нельзя");
        assert_eq!(suggest(&db, 6).await, [2, 3, 4], "frank подписан на alice, а она подписана на bob, carol, dave");
    }
}
