fn recent_logins() -> &'static str {
    "SELECT username FROM users ORDER BY last_login_at DESC NULLS LAST"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn never_logged_in_go_last() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(recent_logins()).await;
        let names = t.col("username");
        assert_eq!(
            names[..9],
            ["Karl", "grace", "frank", "bob_2", "alice", "bob", "ivan", "carol", "judy"],
            "первыми должны идти заходившие, от свежих к старым:\n{t}"
        );
        let mut tail = names[9..].to_vec();
        tail.sort();
        assert_eq!(tail, ["dave", "eve", "heidi"], "те, у кого last_login_at пуст, идут в самом конце:\n{t}");
    }
}
