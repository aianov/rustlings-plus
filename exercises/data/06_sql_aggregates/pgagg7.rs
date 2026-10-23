// Таблица рейтинга авторов: ник, число постов и сумма просмотров по всем пользователям, в том
// числе по тем, кто ещё ничего не написал. Для них должны быть нули. В отчёте у eve и frank
// написано «1 пост», а в колонке просмотров пусто. Порядок по нику.
// TODO: Найди обе ошибки и исправь запрос (столбцы username, posts, views).

fn author_table() -> &'static str {
    "SELECT u.username, count(*) AS posts, sum(p.views) AS views
     FROM users u
     LEFT JOIN posts p ON p.author_id = u.id
     GROUP BY u.id
     ORDER BY u.username"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn zeros_for_silent_users() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(author_table()).await;
        assert_eq!(t.columns, ["username", "posts", "views"], "нужны столбцы username, posts, views");
        assert_eq!(
            t.rows,
            [["alice", "3", "390"], ["bob", "2", "190"], ["carol", "2", "30"], ["dave", "1", "0"], ["eve", "0", "0"], ["frank", "0", "0"]],
            "у eve и frank должно быть 0 постов и 0 просмотров, у dave один пост без посчитанных просмотров (тоже 0):\n{t}"
        );
    }
}
