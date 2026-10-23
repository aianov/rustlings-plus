fn people_and_posts() -> &'static str {
    "SELECT u.username, p.body
     FROM users u
     LEFT JOIN posts p ON p.author_id = u.id
     ORDER BY u.id, p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn users_without_posts_stay() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(people_and_posts()).await;
        assert_eq!(t.columns, ["username", "body"], "нужны столбцы username и body");
        assert_eq!(
            t.rows,
            [
                ["alice", "rust tips"], ["alice", "async"], ["bob", "coffee"], ["bob", "monday"], ["carol", "photos"],
                ["carol", "sunset"], ["dave", "hello"], ["eve", "NULL"], ["frank", "NULL"], ["grace", "NULL"],
            ],
            "у eve, frank и grace постов нет, они должны остаться со значением NULL; у остальных по строке на пост:\n{t}"
        );
    }
}
