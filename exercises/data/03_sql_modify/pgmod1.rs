// Регистрация: в базе должен появиться пользователь с ником zoe и почтой zoe@example.com.
// Остальные поля (статус, очки, время регистрации) база заполняет сама значениями по умолчанию,
// их в запросе быть не должно. Столбцы в запросе перечисляются явно: тогда он переживёт
// появление в таблице новых столбцов.
// Таблицы описаны в seed.sql рядом с задачей.
// TODO: Напиши запрос, который добавляет этого пользователя.

fn register() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn user_is_added_with_defaults() {
        let db = TestDb::with_sql(SEED).await;
        db.run(register()).await;
        let t = db.table("SELECT username, email, status, score, created_at > now() - interval '1 minute' FROM users WHERE username = 'zoe'").await;
        assert_eq!(t.rows, [["zoe", "zoe@example.com", "active", "0", "true"]], "новый пользователь должен получить значения по умолчанию:\n{t}");
    }

    #[tokio::test]
    async fn nobody_else_is_touched() {
        let db = TestDb::with_sql(SEED).await;
        db.run(register()).await;
        assert_eq!(db.table("SELECT id FROM users").await.len(), 7, "в таблице было шесть пользователей, должен добавиться ровно один");
    }
}
