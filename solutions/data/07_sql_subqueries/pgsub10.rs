fn preview_comments() -> &'static str {
    "SELECT p.id AS post_id, c.id AS comment_id
     FROM posts p
     CROSS JOIN LATERAL (
         SELECT id FROM comments
         WHERE post_id = p.id
         ORDER BY created_at DESC, id DESC
         LIMIT 3
     ) c
     ORDER BY p.id, c.id DESC"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn three_latest_per_post() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(preview_comments()).await;
        assert_eq!(t.columns, ["post_id", "comment_id"], "нужны столбцы post_id и comment_id");
        assert_eq!(
            t.rows,
            [["1", "5"], ["1", "4"], ["1", "3"], ["2", "6"], ["3", "17"], ["3", "16"], ["3", "15"], ["6", "8"], ["6", "7"]],
            "по каждому посту не больше трёх свежих комментариев, от новых к старым; LIMIT 3 на весь запрос дал бы три строки всего:\n{t}"
        );
    }

    #[tokio::test]
    async fn latest_means_by_time() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO comments (post_id, author_id, body, created_at) VALUES (2, 3, 'late', '2026-10-09 10:00:00+00'), (2, 4, 'later', '2026-10-09 11:00:00+00')").await;
        let t = db.table(preview_comments()).await;
        let ids: Vec<&str> = t.rows.iter().filter(|r| r[0] == "2").map(|r| r[1].as_str()).collect();
        assert_eq!(ids, ["102", "101", "6"], "у поста 2 теперь три комментария, показываем все, от нового к старому:\n{t}");
    }
}
