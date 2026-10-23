fn likes_overview() -> &'static str {
    "SELECT count(*) AS likes_total, count(DISTINCT user_id) AS unique_likers FROM likes"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn distinct_likers() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(likes_overview()).await;
        assert_eq!(t.columns, ["likes_total", "unique_likers"], "нужны столбцы likes_total и unique_likers");
        assert_eq!(t.rows, [["11", "5"]], "лайков 11, а поставили их пятеро разных людей:\n{t}");
    }
}
