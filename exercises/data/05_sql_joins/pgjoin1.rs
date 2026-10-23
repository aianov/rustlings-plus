// Главная страница показывает посты с именами авторов, а в таблице posts лежит только author_id.
// Имя живёт в users. Нужен список всех постов: id поста, ник автора (столбец называется
// username) и текст поста (body), по возрастанию id поста.
// TODO: Напиши запрос.

fn feed_with_authors() -> &'static str {
    "SELECT id, body FROM posts"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn posts_with_authors() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(feed_with_authors()).await;
        assert_eq!(t.columns, ["id", "username", "body"], "нужны столбцы id, username, body");
        assert_eq!(
            t.rows,
            [["1", "alice", "rust tips"], ["2", "bob", "coffee"], ["3", "alice", "async"], ["4", "carol", "photos"], ["5", "bob", "monday"], ["6", "dave", "hello"], ["7", "carol", "sunset"]],
            "каждому посту нужен его автор, всего семь строк:\n{t}"
        );
    }
}
