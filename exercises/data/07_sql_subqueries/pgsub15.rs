// Страница ветки: корневой комментарий и все ответы на него, на любой глубине. $1 это id
// корня ветки. Столбцы id, depth (корень 0, прямые ответы 1 и так далее) и body. Порядок такой,
// как читает человек: ответ сразу под своим родителем, соседи по id (обход в глубину), а не слоями.
// TODO: Напиши запрос.

fn branch() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn rows(db: &TestDb, root: i64) -> Vec<(i64, i32, String)> {
        let sql = format!("SELECT q.id::bigint, q.depth::int, q.body::text FROM ({}) AS q", branch());
        sqlx::query_as::<_, (i64, i32, String)>(&sql)
            .bind(root)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("branch({root}): {e}"))
    }

    fn row(id: i64, depth: i32, body: &str) -> (i64, i32, String) {
        (id, depth, body.to_string())
    }

    #[tokio::test]
    async fn depth_first_order() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            rows(&db, 10).await,
            [
                row(10, 0, "who owns async?"),
                row(11, 1, "me"),
                row(14, 2, "thanks"),
                row(15, 3, "spam"),
                row(16, 4, "wow"),
                row(13, 1, "same here"),
            ],
            "ветка 10 должна читаться сверху вниз: ответ 13 идёт после всей подветки 11 (14, 15, 16), а не сразу за 11; обход слоями или сортировка по id даст другой порядок"
        );
    }

    #[tokio::test]
    async fn other_branches_are_separate() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(rows(&db, 12).await, [row(12, 0, "great post"), row(17, 1, "yes")], "в ветке 12 корень и один ответ");
        assert_eq!(rows(&db, 1).await, [row(1, 0, "nice"), row(5, 1, "thanks")], "ветка 1 с поста 1: корень и один ответ");
        assert_eq!(rows(&db, 13).await, [row(13, 0, "same here")], "у листа ветка из него одного, глубина считается от переданного корня");
    }
}
