fn post_counters() -> &'static str {
    "SELECT p.id AS post_id, COALESCE(l.n, 0) AS likes, COALESCE(c.n, 0) AS comments, COALESCE(c.chars, 0) AS comment_chars
     FROM posts p
     LEFT JOIN (SELECT post_id, count(*) AS n FROM likes GROUP BY post_id) l ON l.post_id = p.id
     LEFT JOIN (SELECT post_id, count(*) AS n, sum(length(body)) AS chars FROM comments GROUP BY post_id) c ON c.post_id = p.id
     ORDER BY p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn no_multiplication() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(post_counters()).await;
        assert_eq!(t.columns, ["post_id", "likes", "comments", "comment_chars"], "нужны столбцы post_id, likes, comments, comment_chars");
        assert_eq!(
            t.rows,
            [
                ["1", "4", "3", "14"], ["2", "1", "0", "0"], ["3", "2", "1", "4"], ["4", "0", "0", "0"],
                ["5", "3", "2", "8"], ["6", "1", "0", "0"], ["7", "0", "0", "0"], ["8", "0", "0", "0"],
            ],
            "у поста 1 четыре лайка, три комментария и 14 символов в них, а не по 12 и 56; при соединении двух связей один ко многим строки перемножаются:\n{t}"
        );
    }
}
