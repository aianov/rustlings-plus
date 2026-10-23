fn dead_accounts() -> &'static str {
    "SELECT u.username
     FROM users u
     LEFT JOIN posts p ON p.author_id = u.id
     LEFT JOIN likes l ON l.user_id = u.id
     WHERE p.id IS NULL AND l.post_id IS NULL
     ORDER BY u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn no_posts_and_no_likes() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(dead_accounts()).await;
        assert_eq!(t.column(0), ["frank", "grace"], "eve постов не писала, но поставила лайк, поэтому «мёртвые» только frank и grace:\n{t}");
    }
}
