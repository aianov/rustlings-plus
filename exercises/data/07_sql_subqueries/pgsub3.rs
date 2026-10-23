// В списке пользователей каждому нужен счётчик подписчиков. Столбцы: username и followers (сколько
// людей на него подписано, смотри follows.followee_id). Пользователь, у которого подписчиков нет,
// должен остаться в списке с нулём. Порядок по нику.
// TODO: Напиши запрос.

fn with_followers() -> &'static str {
    "SELECT username, 0 AS followers FROM users ORDER BY username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn counts_followers_per_user() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(with_followers()).await;
        assert_eq!(t.columns, ["username", "followers"], "нужны столбцы username и followers");
        assert_eq!(
            t.rows,
            [["alice", "4"], ["bob", "1"], ["carol", "2"], ["dave", "4"], ["eve", "1"], ["frank", "1"], ["grace", "0"]],
            "у grace подписчиков нет, но она должна остаться в списке с нулём:\n{t}"
        );
    }

    #[tokio::test]
    async fn counter_follows_new_data() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO follows VALUES (5, 7), (6, 7)").await;
        let t = db.table(with_followers()).await;
        assert_eq!(t.cell(6, 1), "2", "после двух новых подписок у grace два подписчика:\n{t}");
    }
}
