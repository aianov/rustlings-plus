// Раздел «Кто кого лайкает»: для каждого лайка нужны ник того, кто поставил лайк (liker), ник
// автора поста (author) и текст поста (body). Таблицы: likes (post_id, user_id), posts, users.
// Имена людей лежат в одной и той же таблице users, но нужны обе роли сразу. Порядок: ник
// лайкнувшего, потом id поста.
// TODO: Напиши запрос.

fn who_likes_whom() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn liker_author_post() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(who_likes_whom()).await;
        assert_eq!(t.columns, ["liker", "author", "body"], "нужны столбцы liker, author, body");
        assert_eq!(
            t.rows,
            [
                ["alice", "bob", "coffee"], ["alice", "bob", "monday"], ["bob", "alice", "rust tips"], ["bob", "alice", "async"],
                ["carol", "alice", "rust tips"], ["dave", "alice", "async"], ["eve", "carol", "photos"],
            ],
            "семь лайков, у каждого лайкнувший и автор (это две разные роли одной таблицы):\n{t}"
        );
    }
}
