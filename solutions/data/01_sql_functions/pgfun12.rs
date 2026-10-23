fn joined_on() -> &'static str {
    "SELECT username, (created_at AT TIME ZONE tz)::date AS joined FROM users"
}

fn tokyo_time() -> &'static str {
    "SELECT username, created_at AT TIME ZONE 'Asia/Tokyo' AS local FROM users"
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
    async fn date_in_user_zone() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(joined_on()).await;
        assert_eq!(row(&t, "kenji")[1], "2024-03-02", "для kenji в Токио уже 2 марта:\n{t}");
        assert_eq!(row(&t, "bob")[1], "2024-02-29", "для bob в Нью-Йорке ещё 29 февраля:\n{t}");
        assert_eq!(row(&t, "alice")[1], "2024-01-10");
    }

    #[tokio::test]
    async fn local_wall_clock() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(tokyo_time()).await;
        assert_eq!(row(&t, "kenji")[1], "2024-03-02 08:30:00", "23:30 UTC это 08:30 следующего дня в Токио (UTC+9):\n{t}");
    }
}
