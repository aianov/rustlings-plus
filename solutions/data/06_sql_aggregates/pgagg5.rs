fn posts_per_day() -> &'static str {
    "SELECT date_trunc('day', created_at)::date AS day, count(*) AS posts
     FROM posts
     GROUP BY 1
     ORDER BY 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn grouped_by_day() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(posts_per_day()).await;
        assert_eq!(t.columns, ["day", "posts"], "нужны столбцы day и posts");
        assert_eq!(t.rows, [["2026-09-28", "2"], ["2026-09-29", "1"], ["2026-10-01", "2"], ["2026-10-03", "3"]], "по дням:\n{t}");
    }
}
