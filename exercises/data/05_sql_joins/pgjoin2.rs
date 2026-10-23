// Запрос для админки: id поста, ник автора и время создания поста. Он написан, но база
// отвечает ошибкой про неоднозначную ссылку на столбец: у users и posts одинаково называется
// столбец id. Выходные столбцы должны называться post_id, username, posted_at.
// TODO: Почини запрос.

fn admin_view() -> &'static str {
    "SELECT id, username, created_at
     FROM posts JOIN users ON users.id = posts.author_id
     ORDER BY id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn names_are_unambiguous() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(admin_view()).await;
        assert_eq!(t.columns, ["post_id", "username", "posted_at"], "столбцы должны называться post_id, username, posted_at");
        assert_eq!(t.len(), 7, "по строке на пост:\n{t}");
        assert_eq!(t.rows[0], ["1", "alice", "2026-10-01 10:00:00+00"], "первая строка: пост 1 alice, id и время именно поста:\n{t}");
        assert_eq!(t.rows[5], ["6", "dave", "2026-10-07 07:30:00+00"], "строка поста 6:\n{t}");
    }
}
