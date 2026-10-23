fn latest_per_author() -> &'static str {
    "SELECT DISTINCT ON (author_id) author_id, id, body
     FROM posts
     ORDER BY author_id, created_at DESC"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn newest_post_of_each_author() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(latest_per_author()).await;
        assert_eq!(t.columns, ["author_id", "id", "body"], "нужны столбцы author_id, id, body");
        assert_eq!(
            t.rows,
            [["1", "3", "async"], ["2", "2", "coffee"], ["3", "4", "photos"], ["4", "6", "hello"], ["5", "9", "quiet"]],
            "по одной строке на автора, самый поздний пост по времени (у alice это 3, а не 8):\n{t}"
        );
    }

    #[tokio::test]
    async fn follows_new_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, likes, created_at) VALUES (3, 'later', 0, '2026-10-08 09:00:00+00')").await;
        let t = db.table(latest_per_author()).await;
        assert_eq!(t.cell(2, 2), "later", "после нового поста у carol он должен стать последним:\n{t}");
        assert_eq!(t.len(), 5, "строк по числу авторов, пять");
    }
}
