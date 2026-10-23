fn trending() -> &'static str {
    "SELECT id, likes,
            rank() OVER (ORDER BY likes DESC) AS place,
            dense_rank() OVER (ORDER BY likes DESC) AS dense_place
     FROM posts
     ORDER BY place, id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn ties_share_a_place() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(trending()).await;
        assert_eq!(t.columns, ["id", "likes", "place", "dense_place"], "нужны столбцы id, likes, place, dense_place");
        assert_eq!(
            t.rows,
            [
                ["2", "8", "1", "1"], ["3", "8", "1", "1"], ["5", "8", "1", "1"], ["6", "8", "1", "1"],
                ["1", "5", "5", "2"], ["8", "5", "5", "2"],
                ["4", "3", "7", "3"], ["10", "3", "7", "3"],
                ["9", "1", "9", "4"], ["7", "0", "10", "5"],
            ],
            "четыре поста с восемью лайками делят первое место; после них place пропускает до пятого, а dense_place идёт подряд:\n{t}"
        );
    }

    #[tokio::test]
    async fn places_move_with_new_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, likes, created_at) VALUES (6, 'extra', 8, '2026-10-08 10:00:00+00')").await;
        let t = db.table(trending()).await;
        let row = t.rows.iter().find(|r| r[0] == "9").expect("пост 9 в выдаче");
        assert_eq!((row[2].as_str(), row[3].as_str()), ("10", "4"), "после пятого поста с восемью лайками место поста 9 сдвигается до десятого, а dense_place остаётся четвёртым:\n{t}");
    }
}
