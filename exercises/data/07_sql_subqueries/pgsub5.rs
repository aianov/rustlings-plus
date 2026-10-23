// Две выборки для модерации.
//  - `silent_posts`: посты, под которыми никто не оставил комментарий. Столбцы id, body, порядок по id.
//  - `fans_of`: $1 это id автора, нужны ники других пользователей, которые хотя бы раз
//    комментировали его посты. Каждый ник один раз, сам автор не считается, порядок по нику.
//    (Столбец один: username.)
// TODO: Напиши оба запроса.

fn silent_posts() -> &'static str {
    "SELECT 1"
}

fn fans_of() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn fans(db: &TestDb, author: i64) -> Vec<String> {
        sqlx::query_scalar::<_, String>(fans_of())
            .bind(author)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("fans_of({author}): {e}"))
    }

    #[tokio::test]
    async fn posts_without_comments() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(silent_posts()).await;
        assert_eq!(t.columns, ["id", "body"], "нужны столбцы id и body");
        assert_eq!(
            t.rows,
            [["4", "photos"], ["5", "monday"], ["7", "sunset"], ["8", "night"], ["9", "quiet"], ["10", "tuesday"]],
            "комментариев нет у шести постов:\n{t}"
        );
    }

    #[tokio::test]
    async fn each_fan_once() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            fans(&db, 1).await,
            ["bob", "carol", "dave", "eve", "frank"],
            "под постами alice писали пятеро (bob и dave по несколько раз, каждый должен быть один раз), сама alice не считается"
        );
        assert_eq!(fans(&db, 2).await, ["alice"], "под постами bob комментировала только alice");
        assert!(fans(&db, 3).await.is_empty(), "под постами carol никто не писал");
    }
}
