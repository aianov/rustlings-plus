fn month_start() -> &'static str {
    "SELECT id, DATE_TRUNC('month', created_at) AS month FROM posts WHERE id <= 12"
}

fn weekday() -> &'static str {
    "SELECT id, EXTRACT(dow FROM created_at)::int AS dow FROM posts WHERE id <= 12"
}

fn quarter_of() -> &'static str {
    "SELECT id, EXTRACT(quarter FROM created_at)::int AS q FROM posts WHERE id <= 12"
}

fn current_year() -> &'static str {
    "SELECT EXTRACT(year FROM now())::int AS y"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::{Table, TestDb};

    const SEED: &str = include_str!("seed.sql");

    fn row<'a>(t: &'a Table, key: &str) -> &'a Vec<String> {
        t.rows
            .iter()
            .find(|r| r[0] == key)
            .unwrap_or_else(|| panic!("в результате нет строки с ключом {key}:\n{t}"))
    }

    #[tokio::test]
    async fn truncate_to_month() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(month_start()).await;
        assert_eq!(row(&t, "4")[1], "2026-04-01 00:00:00+00", "пост 4 от 2 апреля, месяц начался 1 апреля:\n{t}");
    }

    #[tokio::test]
    async fn day_of_week() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(weekday()).await;
        assert_eq!(row(&t, "1")[1], "0", "1 марта 2026 это воскресенье, то есть 0:\n{t}");
        assert_eq!(row(&t, "2")[1], "2", "17 марта 2026 это вторник:\n{t}");
    }

    #[tokio::test]
    async fn quarter() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(quarter_of()).await;
        assert_eq!(row(&t, "1")[1], "1", "март это первый квартал:\n{t}");
        assert_eq!(row(&t, "4")[1], "2", "апрель это второй квартал:\n{t}");
        assert_eq!(row(&t, "9")[1], "3", "июль это третий квартал:\n{t}");
    }

    #[tokio::test]
    async fn year_of_now() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(current_year()).await;
        let want = db.table("SELECT EXTRACT(year FROM now())::int").await;
        assert_eq!(t.cell(0, 0), want.cell(0, 0), "ждём текущий год:\n{t}");
    }
}
