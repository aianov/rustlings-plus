// Лайки прилетают пачками. Пост с id 1 набрал 3 лайка, и в этот момент двадцать человек
// одновременно нажали на сердечко. Приложение шлёт на каждое нажатие один и тот же запрос
// (без параметров, число лайков оно не читает). После всех двадцати нажатий счётчик
// likes_count должен вырасти ровно на 20, не потеряв ни одного, а остальные посты остаться как были.
// TODO: Напиши запрос, который обрабатывает одно нажатие.

fn like_post_one() -> &'static str {
    "UPDATE posts SET likes_count = 4 WHERE id = 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");


    #[tokio::test]
    async fn twenty_concurrent_likes_are_all_counted() {
        let db = TestDb::with_sql(SEED).await;
        let mut handles = Vec::new();
        for _ in 0..20 {
            let pool = db.pool.clone();
            handles.push(tokio::spawn(async move {
                sqlx::query(like_post_one()).execute(&pool).await.map(|r| r.rows_affected())
            }));
        }
        for h in handles {
            let affected = h.await.expect("задача упала").unwrap_or_else(|e| panic!("запрос не выполнился: {e}"));
            assert_eq!(affected, 1, "каждое нажатие должно менять ровно один пост");
        }
        let t = db.table("SELECT id, likes_count FROM posts ORDER BY id").await;
        assert_eq!(t.cell(0, 1), "23", "было 3, добавили 20 одновременных лайков:\n{t}");
        assert_eq!(t.column(1)[1..], ["0", "0", "0", "0", "0"], "у остальных постов лайков не прибавилось:\n{t}");
    }
}
