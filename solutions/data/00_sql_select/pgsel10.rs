fn by_country_then_age() -> &'static str {
    "SELECT username, country, age FROM users ORDER BY country ASC, age DESC"
}

fn newest_first() -> &'static str {
    "SELECT username FROM users ORDER BY created_at DESC"
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
