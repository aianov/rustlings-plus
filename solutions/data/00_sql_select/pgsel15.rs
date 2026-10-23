fn over_400_months() -> &'static str {
    "SELECT username, age * 12 AS months FROM users WHERE age * 12 > 400"
}

fn oldest() -> &'static str {
    "SELECT username, age * 12 AS months FROM users ORDER BY months DESC LIMIT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    fn set(mut v: Vec<&str>) -> Vec<&str> {
        v.sort();
        v
    }

    #[tokio::test]
    async fn filter_by_expression() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(over_400_months()).await;
        assert_eq!(
            set(t.col("username")),
            set(vec!["bob", "carol", "frank", "ivan", "Karl"]),
            "ждём пользователей старше 33 лет и 4 месяцев:\n{t}"
        );
        assert_eq!(t.columns, ["username", "months"], "в ответе должны остаться оба столбца");
    }

    #[tokio::test]
    async fn order_by_alias_is_fine() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(oldest()).await;
        assert_eq!(t.col("username"), ["frank"], "самый старший пользователь frank:\n{t}");
    }
}
