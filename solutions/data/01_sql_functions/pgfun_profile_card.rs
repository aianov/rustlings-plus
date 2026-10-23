fn account_age() -> &'static str {
    "SELECT username, EXTRACT(day FROM timestamptz '2026-10-01 00:00:00+00' - created_at)::int AS days FROM users"
}

fn activity() -> &'static str {
    "SELECT username,
            CASE
                WHEN last_login_at IS NULL THEN 'never'
                WHEN last_login_at > timestamptz '2026-10-01 00:00:00+00' - interval '7 days' THEN 'active'
                WHEN last_login_at > timestamptz '2026-10-01 00:00:00+00' - interval '30 days' THEN 'sleeping'
                ELSE 'gone'
            END AS label
     FROM users"
}

fn bio_line() -> &'static str {
    "SELECT username, COALESCE(NULLIF(TRIM(bio), ''), 'no bio') AS bio FROM users"
}

fn display() -> &'static str {
    "SELECT username, COALESCE(NULLIF(TRIM(display_name), ''), username) AS display FROM users"
}

fn joined_local() -> &'static str {
    "SELECT username, (created_at AT TIME ZONE tz)::date AS joined FROM users"
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
    async fn age_of_account() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(account_age()).await;
        assert_eq!(t.columns, ["username", "days"], "нужны столбцы username и days");
        assert_eq!(row(&t, "alice")[1], "994", "полных дней от регистрации alice до 1 октября 2026:\n{t}");
        assert_eq!(row(&t, "heidi")[1], "785");
    }

    #[tokio::test]
    async fn activity_label() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(activity()).await;
        assert_eq!(t.columns, ["username", "label"], "нужны столбцы username и label");
        for (who, want) in [
            ("alice", "active"),
            ("bob", "sleeping"),
            ("carol", "gone"),
            ("dave", "never"),
            ("kenji", "active"),
            ("eve", "gone"),
            ("frank", "gone"),
            ("grace", "sleeping"),
            ("heidi", "never"),
        ] {
            assert_eq!(row(&t, who)[1], want, "метка пользователя {who}: NULL проверяй первым, границы (ровно 7 и ровно 30 суток) уже в следующей категории:\n{t}");
        }
    }

    #[tokio::test]
    async fn bio_fallback() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(bio_line()).await;
        assert_eq!(t.columns, ["username", "bio"], "нужны столбцы username и bio");
        assert_eq!(row(&t, "alice")[1], "Rust and coffee");
        for who in ["bob", "carol", "dave"] {
            assert_eq!(row(&t, who)[1], "no bio", "у {who} биографии нет (NULL, пусто или пробелы):\n{t}");
        }
    }

    #[tokio::test]
    async fn display_name() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(display()).await;
        assert_eq!(t.columns, ["username", "display"], "нужны столбцы username и display");
        assert_eq!(row(&t, "alice")[1], "Alice A.", "пробелы по краям убираем");
        assert_eq!(row(&t, "bob")[1], "bob", "нет имени, показываем ник");
    }

    #[tokio::test]
    async fn local_date() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(joined_local()).await;
        assert_eq!(t.columns, ["username", "joined"], "нужны столбцы username и joined");
        assert_eq!(row(&t, "kenji")[1], "2024-03-02");
        assert_eq!(row(&t, "bob")[1], "2024-02-29");
    }
}
