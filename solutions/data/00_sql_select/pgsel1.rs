fn all_users() -> &'static str {
    "SELECT * FROM users"
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
