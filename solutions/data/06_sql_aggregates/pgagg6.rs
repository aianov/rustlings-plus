fn active_authors() -> &'static str {
    "SELECT u.username, count(*) AS posts
     FROM posts p
     JOIN users u ON u.id = p.author_id
     WHERE p.created_at >= '2026-09-29'
     GROUP BY u.username
     HAVING count(*) >= 2
     ORDER BY u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn fresh_posts_only() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(active_authors()).await;
        assert_eq!(t.columns, ["username", "posts"], "нужны столбцы username и posts");
        assert_eq!(
            t.rows,
            [["bob", "2"], ["carol", "2"]],
            "свежих постов (с 29 сентября) по два у bob и carol; у alice их всего один, остальные старые, хотя всего постов три:\n{t}"
        );
    }
}
