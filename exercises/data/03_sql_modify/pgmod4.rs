// Проверка почты. Пользователи в статусе 'pending' с почтой на @mail.org прошли проверку:
// им нужно поставить статус 'active' и сразу начислить 5 очков за подтверждение.
// Всех остальных (в том числе pending с другой почтой) трогать нельзя.
// TODO: Напиши запрос.

fn confirm_mail_org() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn only_matching_users_change() {
        let db = TestDb::with_sql(SEED).await;
        db.run(confirm_mail_org()).await;
        let t = db.table("SELECT username, status, score FROM users ORDER BY id").await;
        assert_eq!(
            t.rows,
            [
                ["alice", "active", "100"],
                ["bob", "active", "50"],
                ["carol", "banned", "0"],
                ["dave", "active", "20"],
                ["eve", "active", "5"],
                ["frank", "pending", "0"],
            ],
            "изменился должен быть только eve:\n{t}"
        );
    }
}
