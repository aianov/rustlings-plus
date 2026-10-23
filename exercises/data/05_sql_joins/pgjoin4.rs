// Надо показать всех пользователей и их свежие посты, то есть написанные 5 октября 2026 и позже.
// У кого свежих постов нет, тот всё равно должен быть в списке с пустым post_id. Запрос написан,
// но в результате остались только авторы свежих постов, остальные пропали. Порядок: id
// пользователя, потом id поста.
// TODO: Найди, почему пропадают пользователи, и почини.

fn recent_posts() -> &'static str {
    "SELECT u.username, p.id AS post_id
     FROM users u
     LEFT JOIN posts p ON p.author_id = u.id
     WHERE p.created_at >= '2026-10-05'
     ORDER BY u.id, p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn everyone_is_listed() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(recent_posts()).await;
        assert_eq!(
            t.rows,
            [["alice", "3"], ["bob", "NULL"], ["carol", "4"], ["dave", "6"], ["eve", "NULL"], ["frank", "NULL"], ["grace", "NULL"]],
            "все семь пользователей должны быть в списке; свежие посты: 3 (alice), 4 (carol), 6 (dave), у остальных NULL:\n{t}"
        );
    }
}
