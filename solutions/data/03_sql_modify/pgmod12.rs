fn apply_weekly() -> &'static str {
    "UPDATE users SET score = users.score + w.points FROM weekly_scores w WHERE users.id = w.user_id"
}

fn drop_follows_of_banned() -> &'static str {
    "DELETE FROM follows USING users WHERE follows.followee_id = users.id AND users.status = 'banned'"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn scores_are_added_only_to_matching_users() {
        let db = TestDb::with_sql(SEED).await;
        db.run(apply_weekly()).await;
        let t = db.table("SELECT username, score FROM users ORDER BY id").await;
        assert_eq!(
            t.rows,
            [["alice", "110"], ["bob", "55"], ["carol", "0"], ["dave", "20"], ["eve", "7"], ["frank", "0"]],
            "очки добавляются alice, bob и eve; строка с user_id 99 игнорируется:\n{t}"
        );
    }

    #[tokio::test]
    async fn follows_of_banned_are_removed() {
        let db = TestDb::with_sql(SEED).await;
        db.run(drop_follows_of_banned()).await;
        let t = db.table("SELECT follower_id, followee_id FROM follows ORDER BY follower_id, followee_id").await;
        assert_eq!(t.rows, [["1", "2"], ["2", "1"]], "должны остаться подписки не на carol:\n{t}");
    }
}
