fn who_likes_whom() -> &'static str {
    "SELECT liker.username AS liker, author.username AS author, p.body
     FROM likes l
     JOIN users liker ON liker.id = l.user_id
     JOIN posts p ON p.id = l.post_id
     JOIN users author ON author.id = p.author_id
     ORDER BY liker.username, p.id"
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
