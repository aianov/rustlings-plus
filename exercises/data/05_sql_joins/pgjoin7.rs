// Модераторы ищут «мёртвые» аккаунты: пользователей, которые ни разу не написали пост и ни разу
// никого не лайкнули. Нужен столбец username, по алфавиту. Выгружать посты и лайки в приложение
// и сравнивать там не нужно, база сделает это сама.
// TODO: Напиши запрос.

fn dead_accounts() -> &'static str {
    "SELECT username FROM users ORDER BY username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn no_posts_and_no_likes() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(dead_accounts()).await;
        assert_eq!(t.column(0), ["frank", "grace"], "eve постов не писала, но поставила лайк, поэтому «мёртвые» только frank и grace:\n{t}");
    }
}
