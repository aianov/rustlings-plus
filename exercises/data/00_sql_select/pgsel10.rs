// Без ORDER BY порядок строк не определён: сегодня так, завтра иначе. Хочешь порядок, проси его.
// Ключей может быть несколько: по второму ключу упорядочиваются строки с равным первым.
// У каждого ключа своё направление: ASC (по умолчанию, по возрастанию) или DESC.

fn by_country_then_age() -> &'static str {
    // TODO: по стране по алфавиту, внутри страны самые старшие первыми.
    // Верни столбцы username, country, age.
    "SELECT username, country, age FROM users"
}

fn newest_first() -> &'static str {
    // TODO: ники от самых недавно зарегистрированных (по created_at) к самым давним
    "SELECT username FROM users"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn two_keys() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(by_country_then_age()).await;
        assert_eq!(
            t.col("username"),
            ["Karl", "carol", "alice", "grace", "heidi", "dave", "frank", "ivan", "bob", "judy", "eve", "bob_2"],
            "порядок: страна по возрастанию, внутри страны возраст по убыванию:\n{t}"
        );
    }

    #[tokio::test]
    async fn by_date_descending() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(newest_first()).await;
        assert_eq!(
            t.col("username"),
            ["dave", "eve", "carol", "Karl", "judy", "ivan", "heidi", "grace", "frank", "bob", "bob_2", "alice"],
            "первыми должны идти самые поздние регистрации:\n{t}"
        );
    }
}
