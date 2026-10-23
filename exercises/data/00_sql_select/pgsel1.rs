// Таблица это набор строк: одна строка, один пользователь. У всех строк одинаковые столбцы:
// id, username, email и так далее. Тест создаёт для себя пустую схему, заливает в неё seed.sql
// (загляни в него, там вся тестовая соцсеть), выполняет твой запрос и смотрит на результат.

fn all_users() -> &'static str {
    // TODO: Верни запрос, который достаёт всех пользователей и все их столбцы.
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn returns_every_user() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(all_users()).await;
        assert_eq!(t.len(), 12, "в таблице users 12 пользователей, а запрос вернул {}:\n{t}", t.len());
    }

    #[tokio::test]
    async fn returns_every_column() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(all_users()).await;
        assert_eq!(
            t.columns,
            ["id", "username", "email", "country", "age", "status", "created_at", "last_login_at"],
            "нужны все столбцы таблицы users, в том порядке, в котором они объявлены"
        );
    }
}
