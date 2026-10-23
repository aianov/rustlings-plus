fn post_number_in_author() -> &'static str {
    "SELECT author_id, id, row_number() OVER (PARTITION BY author_id ORDER BY created_at, id) AS nth
     FROM posts
     ORDER BY author_id, nth"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn numbering_restarts_for_each_author() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(post_number_in_author()).await;
        assert_eq!(t.columns, ["author_id", "id", "nth"], "нужны столбцы author_id, id, nth");
        assert_eq!(
            t.rows,
            [
                ["1", "1", "1"], ["1", "8", "2"], ["1", "3", "3"],
                ["2", "10", "1"], ["2", "5", "2"], ["2", "2", "3"],
                ["3", "7", "1"], ["3", "4", "2"],
                ["4", "6", "1"], ["5", "9", "1"],
            ],
            "у каждого автора нумерация начинается с единицы и идёт по времени, а не по id (у bob первый пост 10, а не 2):\n{t}"
        );
    }

    #[tokio::test]
    async fn new_post_shifts_numbers() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, likes, created_at) VALUES (5, 'earlier', 0, '2026-09-30 10:00:00+00')").await;
        let t = db.table(post_number_in_author()).await;
        let eve: Vec<(&str, &str)> = t.rows.iter().filter(|r| r[0] == "5").map(|r| (r[1].as_str(), r[2].as_str())).collect();
        assert_eq!(eve, [("11", "1"), ("9", "2")], "новый ранний пост eve стал первым, прежний вторым:\n{t}");
    }
}
