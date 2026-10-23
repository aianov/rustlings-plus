// LIMIT n отдаёт первые n строк, OFFSET m пропускает m строк перед этим. Вместе они дают
// страницы. Страницы имеют смысл только вместе с ORDER BY: «первые пять» из неупорядоченного
// набора это пять случайных строк.

fn newest_three_posts() -> &'static str {
    // TODO: id трёх самых свежих постов (по created_at), свежий первым
    "SELECT id FROM posts"
}

fn third_page() -> &'static str {
    // TODO: третья страница постов, по четыре на странице, порядок по id
    "SELECT id FROM posts"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn limit_only() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(newest_three_posts()).await;
        assert_eq!(t.col("id"), ["13", "8", "3"], "три самых свежих поста, свежий первым:\n{t}");
    }

    #[tokio::test]
    async fn limit_with_offset() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(third_page()).await;
        assert_eq!(t.col("id"), ["9", "10", "11", "12"], "на первых двух страницах 8 постов, третья начинается с девятого:\n{t}");
    }
}
