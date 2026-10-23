// После сбоя импорта в базе оказались испорченные данные: комментарий 30 ответ на 31, а 31
// ответ на 30 (и ещё 32 ответ на 31). Страница ветки для комментария 30 висит, пока её не убьёт
// таймаут. Здоровые ветки работают как надо, и они должны работать так же. Ветка испорченного
// корня должна завершаться и содержать каждый комментарий по одному разу: 30, 31, 32.
// TODO: Найди, почему запрос не заканчивается, и исправь.

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

    const BROKEN: &str = "
        INSERT INTO comments (id, post_id, author_id, parent_id, body) OVERRIDING SYSTEM VALUE VALUES
            (30, 9, 5, NULL, 'loop a'), (31, 9, 1, NULL, 'loop b'), (32, 9, 2, NULL, 'loop c');
        UPDATE comments SET parent_id = 31 WHERE id = 30;
        UPDATE comments SET parent_id = 30 WHERE id = 31;
        UPDATE comments SET parent_id = 31 WHERE id = 32;
    ";

    async fn rows(db: &TestDb, root: i64) -> Result<Vec<(i64, i32, String)>, String> {
        let sql = format!("SELECT q.id::bigint, q.depth::int, q.body::text FROM ({}) AS q", branch());
        let mut tx = db.pool.begin().await.expect("транзакция");
        sqlx::query("SET LOCAL statement_timeout = '3s'").execute(&mut *tx).await.expect("таймаут");
        let result = sqlx::query_as::<_, (i64, i32, String)>(&sql)
            .bind(root)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| e.to_string());
        tx.rollback().await.expect("откат транзакции");
        result
    }

    fn row(id: i64, depth: i32, body: &str) -> (i64, i32, String) {
        (id, depth, body.to_string())
    }

    #[tokio::test]
    async fn healthy_branch_still_works() {
        let db = TestDb::with_sql(SEED).await;
        let got = rows(&db, 10).await.unwrap_or_else(|e| panic!("здоровая ветка 10 не читается: {e}"));
        let ids: Vec<i64> = got.iter().map(|r| r.0).collect();
        assert_eq!(ids, [10, 11, 14, 15, 16, 13], "здоровая ветка должна читаться как раньше, в порядке обхода в глубину");
    }

    #[tokio::test]
    async fn broken_branch_terminates_and_has_no_repeats() {
        let db = TestDb::with_sql(SEED).await;
        db.run(BROKEN).await;
        let got = rows(&db, 30)
            .await
            .unwrap_or_else(|e| panic!("запрос не завершился за 3 секунды или упал ({e}): рекурсия ходит по кругу 30, 31, 30, 31..."));
        assert_eq!(
            got,
            [row(30, 0, "loop a"), row(31, 1, "loop b"), row(32, 2, "loop c")],
            "в испорченной ветке нужны 30, 31 и 32 по одному разу; повтор означает, что цикл разорван слишком поздно"
        );
    }
}
