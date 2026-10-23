fn numbered_feed() -> &'static str {
    "SELECT row_number() OVER (ORDER BY created_at DESC, id) AS n, id, body
     FROM posts
     ORDER BY n"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn numbers_follow_freshness() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(numbered_feed()).await;
        assert_eq!(t.columns, ["n", "id", "body"], "нужны столбцы n, id, body");
        let ids: Vec<&str> = t.col("id");
        assert_eq!(ids, ["6", "9", "4", "3", "8", "2", "5", "7", "1", "10"], "номер один у самого свежего поста (6), дальше по убыванию created_at:\n{t}");
        let ns: Vec<&str> = t.col("n");
        assert_eq!(ns, ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"], "номера идут подряд с единицы:\n{t}");
    }

    #[tokio::test]
    async fn new_post_takes_number_one() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, likes, created_at) VALUES (7, 'fresh', 0, '2026-10-09 10:00:00+00')").await;
        let t = db.table(numbered_feed()).await;
        assert_eq!((t.cell(0, 0), t.cell(0, 2)), ("1", "fresh"), "самый свежий пост должен получить номер 1:\n{t}");
        assert_eq!(t.len(), 11, "строк стало одиннадцать");
    }
}
