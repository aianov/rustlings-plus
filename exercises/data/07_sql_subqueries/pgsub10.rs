// Под каждым постом в ленте показываются превью трёх последних комментариев. Нужны пары
// post_id и comment_id: для каждого поста до трёх его самых свежих комментариев (по created_at,
// при равенстве по id), от свежего к старому. Посты без комментариев в результат не входят.
// Порядок: post_id по возрастанию, внутри поста от свежих к старым. Берём все посты сразу.
// TODO: Напиши запрос. Срез в три строки нужен на каждый пост, а не на весь запрос.

fn preview_comments() -> &'static str {
    "SELECT 1"
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
