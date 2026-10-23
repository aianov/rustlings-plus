fn crumbs() -> &'static str {
    "WITH RECURSIVE up AS (
         SELECT id, parent_id, body, 0 AS hops FROM comments WHERE id = $1
         UNION ALL
         SELECT c.id, c.parent_id, c.body, up.hops + 1
         FROM comments c JOIN up ON c.id = up.parent_id
     )
     SELECT id, body FROM up ORDER BY hops DESC"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn chain(db: &TestDb, id: i64) -> Vec<(i64, String)> {
        sqlx::query_as::<_, (i64, String)>(crumbs())
            .bind(id)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("crumbs({id}): {e}"))
    }

    fn pair(id: i64, body: &str) -> (i64, String) {
        (id, body.to_string())
    }

    #[tokio::test]
    async fn walks_up_to_the_root() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            chain(&db, 16).await,
            [pair(10, "who owns async?"), pair(11, "me"), pair(14, "thanks"), pair(15, "spam"), pair(16, "wow")],
            "от комментария 16 до корня 10 четыре шага вверх; корень должен идти первым"
        );
        assert_eq!(chain(&db, 17).await, [pair(12, "great post"), pair(17, "yes")], "цепочка из двух: корень 12 и ответ 17");
    }

    #[tokio::test]
    async fn root_has_a_chain_of_one() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(chain(&db, 10).await, [pair(10, "who owns async?")], "у корня цепочка из него одного");
        assert!(chain(&db, 999).await.is_empty(), "несуществующий комментарий: пустой результат");
    }
}
