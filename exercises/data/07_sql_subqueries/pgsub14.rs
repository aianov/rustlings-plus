// Страница отдельного ответа показывает «хлебные крошки»: цепочку от корневого комментария
// до этого ответа. $1 это id комментария. Нужны столбцы id и body, корень первым, сам комментарий
// последним. Глубина дерева заранее неизвестна.
// TODO: Напиши запрос. Здесь нужна рекурсия: у каждого комментария есть ссылка parent_id.

fn crumbs() -> &'static str {
    "SELECT id, body FROM comments WHERE id = $1"
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
