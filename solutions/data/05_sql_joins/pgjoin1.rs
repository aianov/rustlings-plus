fn feed_with_authors() -> &'static str {
    "SELECT p.id, u.username, p.body
     FROM posts p
     JOIN users u ON u.id = p.author_id
     ORDER BY p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn posts_with_authors() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(feed_with_authors()).await;
        assert_eq!(t.columns, ["id", "username", "body"], "нужны столбцы id, username, body");
        assert_eq!(
            t.rows,
            [["1", "alice", "rust tips"], ["2", "bob", "coffee"], ["3", "alice", "async"], ["4", "carol", "photos"], ["5", "bob", "monday"], ["6", "dave", "hello"], ["7", "carol", "sunset"]],
            "каждому посту нужен его автор, всего семь строк:\n{t}"
        );
    }
}
