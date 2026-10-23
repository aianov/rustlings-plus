// Админский график «посты по дням» за первую неделю октября 2026 (с 1 по 7 включительно).
// Дни без постов тоже должны быть на графике. Ряд дат создаёт generate_series, а сами посты
// пристыковываются к нему. Нужны столбцы day (дата) и post_id; день, в котором было два
// поста, занимает две строки. Порядок: день, потом id поста. Считать посты агрегатом будем в главе про агрегаты.
// TODO: Напиши запрос.

fn posts_by_day() -> &'static str {
    "SELECT created_at::date AS day, id AS post_id
     FROM posts
     ORDER BY day, post_id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn empty_days_are_present() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(posts_by_day()).await;
        assert_eq!(t.columns, ["day", "post_id"], "нужны столбцы day и post_id");
        assert_eq!(
            t.rows,
            [
                ["2026-10-01", "1"], ["2026-10-01", "7"], ["2026-10-02", "5"], ["2026-10-03", "2"], ["2026-10-04", "NULL"],
                ["2026-10-05", "3"], ["2026-10-05", "4"], ["2026-10-06", "NULL"], ["2026-10-07", "6"],
            ],
            "в 4 и 6 октября постов не было, но дни должны остаться (с NULL):\n{t}"
        );
    }
}
