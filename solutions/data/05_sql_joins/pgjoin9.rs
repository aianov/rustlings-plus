fn friends() -> &'static str {
    "SELECT a.username AS friend_a, b.username AS friend_b
     FROM follows f1
     JOIN follows f2 ON f2.follower_id = f1.followee_id AND f2.followee_id = f1.follower_id
     JOIN users a ON a.id = f1.follower_id
     JOIN users b ON b.id = f1.followee_id
     WHERE f1.follower_id < f1.followee_id
     ORDER BY a.username, b.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn each_pair_once() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(friends()).await;
        assert_eq!(t.columns, ["friend_a", "friend_b"], "нужны столбцы friend_a и friend_b");
        assert_eq!(
            t.rows,
            [["alice", "bob"], ["alice", "carol"], ["dave", "eve"]],
            "взаимных пар три; каждая должна быть один раз, а не в обе стороны:\n{t}"
        );
    }
}
