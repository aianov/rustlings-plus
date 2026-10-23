fn branch() -> &'static str {
    "WITH RECURSIVE down AS (
         SELECT id, 0 AS depth, body, ARRAY[id] AS path FROM comments WHERE id = $1
         UNION ALL
         SELECT c.id, d.depth + 1, c.body, d.path || c.id
         FROM comments c JOIN down d ON c.parent_id = d.id
     )
     SELECT id, depth, body FROM down ORDER BY path"
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
