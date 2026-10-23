fn follow_two_four() -> &'static str {
    "INSERT INTO follows (follower_id, followee_id) VALUES (2, 4) ON CONFLICT DO NOTHING"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn repeat_is_harmless() {
        let db = TestDb::with_sql(SEED).await;
        db.run(follow_two_four()).await;
        db.run(follow_two_four()).await;
        db.run(follow_two_four()).await;
        let t = db.table("SELECT follower_id, followee_id FROM follows WHERE follower_id = 2 AND followee_id = 4").await;
        assert_eq!(t.len(), 1, "после трёх нажатий подписка должна быть одна:\n{t}");
    }

    #[tokio::test]
    async fn other_follows_are_untouched() {
        let db = TestDb::with_sql(SEED).await;
        db.run(follow_two_four()).await;
        assert_eq!(db.table("SELECT follower_id FROM follows").await.len(), 5, "было 4 подписки, добавилась одна");
    }
}
