// Под постами в ленте показываются счётчики лайков и комментариев. После недавней правки цифры
// стали невозможными: у поста 3 три лайка и восемь комментариев, а лента показывает по 24 там
// и там. Нужны все посты по порядку id, столбцы post_id, likes, comments. Каждую связь надо
// свернуть до одной строки на пост раньше, чем связи встретятся.
// TODO: Найди причину и исправь, чтобы цифры совпали с таблицами.

fn post_counters() -> &'static str {
    "SELECT p.id AS post_id, count(l.user_id) AS likes, count(c.id) AS comments
     FROM posts p
     LEFT JOIN likes l ON l.post_id = p.id
     LEFT JOIN comments c ON c.post_id = p.id
     GROUP BY p.id
     ORDER BY p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn counters_match_tables() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(post_counters()).await;
        assert_eq!(t.columns, ["post_id", "likes", "comments"], "нужны столбцы post_id, likes, comments");
        assert_eq!(
            t.rows,
            [["1", "2", "5"], ["2", "1", "1"], ["3", "3", "8"], ["4", "1", "0"], ["5", "0", "0"], ["6", "2", "2"], ["7", "0", "0"], ["8", "0", "0"], ["9", "0", "0"], ["10", "0", "0"]],
            "лайки и комментарии нужно считать по отдельности: две связи в одном JOIN перемножаются, а посты без лайков или комментариев должны остаться с нулями:\n{t}"
        );
    }
}
