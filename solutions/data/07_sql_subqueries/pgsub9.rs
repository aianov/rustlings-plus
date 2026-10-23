fn above_average_authors() -> &'static str {
    "WITH post_likes AS (
         SELECT p.author_id, count(l.user_id) AS n
         FROM posts p
         LEFT JOIN likes l ON l.post_id = p.id
         GROUP BY p.id, p.author_id
     ),
     author_likes AS (
         SELECT author_id, sum(n) AS likes FROM post_likes GROUP BY author_id
     ),
     avg_likes AS (
         SELECT avg(likes) AS a FROM author_likes
     )
     SELECT u.username, al.likes
     FROM author_likes al
     JOIN users u ON u.id = al.author_id
     CROSS JOIN avg_likes
     WHERE al.likes > avg_likes.a
     ORDER BY al.likes DESC, u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn authors_above_the_average() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(above_average_authors()).await;
        assert_eq!(t.columns, ["username", "likes"], "нужны столбцы username и likes");
        assert_eq!(
            t.rows,
            [["alice", "5"], ["dave", "2"]],
            "лайков по авторам: alice 5, dave 2, bob 1, carol 1, eve 0; среднее 1.8 (eve входит с нулём), выше него alice и dave:\n{t}"
        );
    }

    #[tokio::test]
    async fn average_moves_with_the_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO likes VALUES (10, 1), (10, 2), (10, 3), (10, 4), (10, 5), (10, 6), (10, 7)").await;
        let t = db.table(above_average_authors()).await;
        assert_eq!(
            t.rows,
            [["bob", "8"], ["alice", "5"]],
            "после семи лайков на пост bob у него 8, остальные: 5, 2, 1, 0 при среднем 3.2; выше bob и alice:\n{t}"
        );
    }
}
