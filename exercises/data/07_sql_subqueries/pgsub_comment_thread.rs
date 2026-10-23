// Ветка комментариев под постом, как её рисует приложение. Функция `thread`, параметры:
// $1 id поста, $2 максимальная глубина (тип integer; корни имеют глубину 0, глубже $2 не показываем).
// Столбцы: id, depth, body, replies. Правила:
//  - порядок обхода в глубину: ответ сразу под родителем, соседи по id;
//  - удалённые комментарии (deleted = true) показываются, но с текстом '[удалено]', а их ответы остаются;
//  - replies это число прямых ответов на комментарий в базе, даже если сами ответы глубже предела;
//  - предел глубины не должен менять числа replies у видимых комментариев.
// TODO: Напиши запрос.

fn thread() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn rows(db: &TestDb, post: i64, max_depth: i32) -> Vec<(i64, i32, String, i64)> {
        let sql = format!("SELECT q.id::bigint, q.depth::int, q.body::text, q.replies::bigint FROM ({}) AS q", thread());
        sqlx::query_as::<_, (i64, i32, String, i64)>(&sql)
            .bind(post)
            .bind(max_depth)
            .fetch_all(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("thread({post}, {max_depth}): {e}"))
    }

    fn row(id: i64, depth: i32, body: &str, replies: i64) -> (i64, i32, String, i64) {
        (id, depth, body.to_string(), replies)
    }

    #[tokio::test]
    async fn full_thread_in_reading_order() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            rows(&db, 3, 10).await,
            [
                row(10, 0, "who owns async?", 2),
                row(11, 1, "me", 1),
                row(14, 2, "thanks", 1),
                row(15, 3, "[удалено]", 1),
                row(16, 4, "wow", 0),
                row(13, 1, "same here", 0),
                row(12, 0, "great post", 1),
                row(17, 1, "yes", 0),
            ],
            "ветка поста 3: обход в глубину, у удалённого 15 текст заменён, но ответ 16 остался"
        );
    }

    #[tokio::test]
    async fn depth_limit_keeps_reply_counts() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(
            rows(&db, 3, 2).await,
            [
                row(10, 0, "who owns async?", 2),
                row(11, 1, "me", 1),
                row(14, 2, "thanks", 1),
                row(13, 1, "same here", 0),
                row(12, 0, "great post", 1),
                row(17, 1, "yes", 0),
            ],
            "при пределе 2 комментарии глубже скрыты, но у 14 по-прежнему один ответ: число ответов считается по базе, а не по показанным строкам"
        );
        let only_roots = rows(&db, 3, 0).await;
        assert_eq!(only_roots, [row(10, 0, "who owns async?", 2), row(12, 0, "great post", 1)], "при пределе 0 только корни");
    }

    #[tokio::test]
    async fn flat_and_empty_posts() {
        let db = TestDb::with_sql(SEED).await;
        let ids: Vec<i64> = rows(&db, 1, 5).await.into_iter().map(|r| r.0).collect();
        assert_eq!(ids, [1, 5, 2, 3, 4], "пост 1: ответ 5 стоит под своим родителем 1, остальные корни по id");
        assert!(rows(&db, 4, 5).await.is_empty(), "у поста 4 нет комментариев");
    }
}
