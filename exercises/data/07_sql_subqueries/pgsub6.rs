// В профиле пользователя показывается его самый свежий пост. Нужны ники и тексты свежих постов
// каждого автора (столбцы username, body), по одному на автора, порядок по нику. Свежий значит
// по времени создания, а не по номеру: id и created_at в таблице не совпадают по порядку.
// Пользователи без постов в список не входят.
// TODO: Напиши запрос с коррелированным подзапросом.

fn latest_posts() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn newest_by_time_not_by_id() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(latest_posts()).await;
        assert_eq!(t.columns, ["username", "body"], "нужны столбцы username и body");
        assert_eq!(
            t.rows,
            [["alice", "async"], ["bob", "coffee"], ["carol", "photos"], ["dave", "hello"], ["eve", "quiet"]],
            "свежий пост это самый поздний по created_at, а не с наибольшим id (у alice это async, а не night):\n{t}"
        );
    }

    #[tokio::test]
    async fn new_post_becomes_latest() {
        let db = TestDb::with_sql(SEED).await;
        db.run("INSERT INTO posts (author_id, body, created_at) VALUES (2, 'fresh', '2026-10-09 10:00:00+00')").await;
        let t = db.table(latest_posts()).await;
        assert_eq!(t.cell(1, 1), "fresh", "после нового поста bob должен показывать его:\n{t}");
        assert_eq!(t.len(), 5, "по строке на автора, без дублей:\n{t}");
    }
}
