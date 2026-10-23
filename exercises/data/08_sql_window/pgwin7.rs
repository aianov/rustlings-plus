// Нумерация подписок на alice (followee_id = 1): первая, вторая и так далее по дню подписки.
// Нужны столбцы day (дата) и nth. Сейчас числа получаются 2, 2, 3, 6, 6, 6, хотя подписок
// шесть, и номера должны идти 1, 2, 3, 4, 5, 6. Порядок выдачи: day, затем nth.
// TODO: Найди, почему равные дни получают одинаковый номер, и почини.

fn numbered_follows() -> &'static str {
    "SELECT created_at::date AS day, count(*) OVER (ORDER BY created_at::date) AS nth
     FROM follows
     WHERE followee_id = 1
     ORDER BY day, nth"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn numbers_go_one_by_one() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(numbered_follows()).await;
        assert_eq!(t.columns, ["day", "nth"], "нужны столбцы day и nth");
        assert_eq!(
            t.rows,
            [["2026-10-01", "1"], ["2026-10-01", "2"], ["2026-10-02", "3"], ["2026-10-04", "4"], ["2026-10-04", "5"], ["2026-10-04", "6"]],
            "подписок шесть, номера 1..6 без повторов: рамка по умолчанию склеивает строки с равным днём в одну:\n{t}"
        );
    }

    #[tokio::test]
    async fn a_new_follow_extends_the_list() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO users (username) VALUES ('heidi'); INSERT INTO follows VALUES (8, 1, '2026-10-02 20:00:00+00')").await;
        let t = db.table(numbered_follows()).await;
        let nths: Vec<&str> = t.col("nth");
        assert_eq!(nths, ["1", "2", "3", "4", "5", "6", "7"], "семь подписок, номера подряд:\n{t}");
    }
}
