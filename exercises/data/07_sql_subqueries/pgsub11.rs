// Под каждым постом ленты подпись «последний комментарий: ник». Запрос написан, но в ленте
// остались только посты с комментариями, а нужны все десять (у молчащих подпись пустая, NULL).
// Столбцы post_id, last_commenter, порядок по id поста. Последний это самый свежий по created_at,
// при равенстве старший id.
// TODO: Почини запрос.

fn last_commenters() -> &'static str {
    "SELECT p.id AS post_id, l.username AS last_commenter
     FROM posts p
     CROSS JOIN LATERAL (
         SELECT u.username FROM comments c JOIN users u ON u.id = c.author_id
         WHERE c.post_id = p.id
         ORDER BY c.created_at DESC, c.id DESC
         LIMIT 1
     ) l
     ORDER BY p.id"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn every_post_is_listed() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(last_commenters()).await;
        assert_eq!(t.columns, ["post_id", "last_commenter"], "нужны столбцы post_id и last_commenter");
        assert_eq!(t.len(), 10, "в ленте десять постов, и молчащие тоже нужны (с NULL):\n{t}");
        assert_eq!(
            t.rows.iter().map(|r| (r[0].as_str(), r[1].as_str())).collect::<Vec<_>>(),
            [("1", "carol"), ("2", "alice"), ("3", "bob"), ("4", "NULL"), ("5", "NULL"), ("6", "bob"), ("7", "NULL"), ("8", "NULL"), ("9", "NULL"), ("10", "NULL")],
            "последние комментаторы: carol, alice, bob и bob у постов 1, 2, 3, 6, у остальных NULL:\n{t}"
        );
    }

    #[tokio::test]
    async fn new_comment_replaces_label() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO comments (post_id, author_id, body, created_at) VALUES (4, 7, 'first!', '2026-10-09 10:00:00+00')").await;
        let t = db.table(last_commenters()).await;
        assert_eq!(t.cell(3, 1), "grace", "у поста 4 появился комментарий grace:\n{t}");
    }
}
