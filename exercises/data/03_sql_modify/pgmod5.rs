// Поддержка просит отключить аккаунт dave (поставить ему статус 'disabled') и больше никого.
// Ночью выполнили скрипт ниже, а утром написали все пользователи: у них отключён вход.
// Проверь, что происходит с таблицей, и исправь запрос так, чтобы пострадал только dave.
// TODO: Почини запрос.

fn disable_dave() -> &'static str {
    "UPDATE users SET status = 'disabled'"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn only_dave_is_disabled() {
        let db = TestDb::with_sql(SEED).await;
        let res = sqlx::query(disable_dave()).execute(&db.pool).await.unwrap_or_else(|e| panic!("запрос не выполнился: {e}"));
        assert_eq!(res.rows_affected(), 1, "затронутых строк должно быть ровно одна, а их {}: UPDATE без WHERE меняет всю таблицу", res.rows_affected());
        let t = db.table("SELECT username, status FROM users WHERE status = 'disabled'").await;
        assert_eq!(t.rows, [["dave", "disabled"]], "отключён должен быть только dave:\n{t}");
    }
}
