// Счётчик likes_count у постов хранится рядом с постом для быстрого показа, а правда в таблице likes. После
// пары инцидентов счётчики разошлись с реальностью. Сверка: пересчитать число лайков каждого поста по таблице
// likes и исправить только те посты, где счётчик неверен, включая посты, у которых лайков нет вовсе, а счётчик
// не ноль. Запрос должен вернуть id исправленных постов.
// TODO: Верни один UPDATE ... RETURNING id.

fn reconcile() -> &'static str {
    "SELECT 1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = "CREATE TABLE posts (id bigint PRIMARY KEY, likes_count bigint NOT NULL);
        CREATE TABLE likes (post_id bigint NOT NULL, user_id bigint NOT NULL, PRIMARY KEY (post_id, user_id));
        INSERT INTO posts VALUES (1, 3), (2, 5), (3, 0), (4, 2), (5, 1);
        INSERT INTO likes VALUES (1, 10), (1, 11), (1, 12), (2, 10), (2, 11), (4, 10), (4, 11), (4, 12)";

    #[tokio::test]
    async fn only_wrong_counters_are_fixed() {
        let db = TestDb::with_sql(SEED).await;
        let mut fixed: Vec<i64> = sqlx::query_scalar::<_, i64>(reconcile()).fetch_all(&db.pool).await.expect("сверка");
        fixed.sort_unstable();
        assert_eq!(fixed, [2, 4, 5], "исправлены только неверные: 2 (5 вместо 2), 4 (2 вместо 3), 5 (1 при нуле лайков)");
        let t = db.table("SELECT id, likes_count FROM posts ORDER BY id").await;
        assert_eq!(t.rows, [["1", "3"], ["2", "2"], ["3", "0"], ["4", "3"], ["5", "0"]], "счётчики совпадают с таблицей likes\n{t}");
        let again: Vec<i64> = sqlx::query_scalar::<_, i64>(reconcile()).fetch_all(&db.pool).await.expect("повтор");
        assert!(again.is_empty(), "повторная сверка ничего не меняет");
    }
}
