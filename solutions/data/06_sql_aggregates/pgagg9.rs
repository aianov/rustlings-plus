fn likes_report() -> &'static str {
    "SELECT u.username, count(*) AS total, count(*) FILTER (WHERE l.created_at >= '2026-10-01') AS recent
     FROM likes l
     JOIN posts p ON p.id = l.post_id
     JOIN users u ON u.id = p.author_id
     GROUP BY u.id
     ORDER BY u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn total_and_recent() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(likes_report()).await;
        assert_eq!(t.columns, ["username", "total", "recent"], "нужны столбцы username, total, recent");
        assert_eq!(
            t.rows,
            [["alice", "8", "5"], ["bob", "3", "2"]],
            "alice получила 8 лайков, из них 5 в октябре; bob 3 и 2; общий итог не должен зависеть от даты:\n{t}"
        );
    }
}
