fn post_counters() -> &'static str {
    "SELECT p.id AS post_id, COALESCE(l.n, 0) AS likes, COALESCE(c.n, 0) AS comments
     FROM posts p
     LEFT JOIN (SELECT post_id, count(*) AS n FROM likes GROUP BY post_id) l ON l.post_id = p.id
     LEFT JOIN (SELECT post_id, count(*) AS n FROM comments GROUP BY post_id) c ON c.post_id = p.id
     ORDER BY p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn counters_match_tables() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(post_counters()).await;
        assert_eq!(t.columns, ["post_id", "likes", "comments"], "нужны столбцы post_id, likes, comments");
        assert_eq!(
            t.rows,
            [["1", "2", "5"], ["2", "1", "1"], ["3", "3", "8"], ["4", "1", "0"], ["5", "0", "0"], ["6", "2", "2"], ["7", "0", "0"], ["8", "0", "0"], ["9", "0", "0"], ["10", "0", "0"]],
            "лайки и комментарии нужно считать по отдельности: две связи в одном JOIN перемножаются, а посты без лайков или комментариев должны остаться с нулями:\n{t}"
        );
    }
}
