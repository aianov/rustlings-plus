// Нужны только контакты: ник и почта, остальные столбцы никому не интересны. Лишние столбцы
// это лишние байты по сети и лишние места, где код может сломаться при изменении таблицы.

fn contacts() -> &'static str {
    // TODO: Достань из users только username и email, именно в таком порядке.
    "SELECT * FROM users"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn only_requested_columns() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(contacts()).await;
        assert_eq!(t.columns, ["username", "email"], "в ответе должны быть ровно username и email, по порядку");
    }

    #[tokio::test]
    async fn all_rows_present() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(contacts()).await;
        assert_eq!(t.len(), 12, "WHERE не нужен, пользователей должно быть 12:\n{t}");
        let alice = vec!["alice".to_string(), "alice@example.com".to_string()];
        assert!(t.rows.contains(&alice), "в результате нет строки alice / alice@example.com:\n{t}");
    }
}
