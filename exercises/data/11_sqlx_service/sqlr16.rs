// Список постов с сортировкой по параметру `sort` из запроса клиента: `/posts?sort=likes&desc=1`.
// Сейчас имя столбца вклеивается в SQL текстом, и клиент может написать туда что угодно.
// Допустимы только три ключа: `id`, `created_at`, `likes` (это столбец likes_count). Всё остальное
// это ошибка `BadSort`, в базу такая строка не попадает. При равных значениях порядок
// определяет id по возрастанию, чтобы страницы не прыгали.
// TODO: Сделай сортировку безопасной.

use sqlx::PgPool;

#[derive(Debug)]
enum ListError {
    BadSort,
    Db(sqlx::Error),
}

async fn list_posts(pool: &PgPool, sort: &str, desc: bool) -> Result<Vec<i64>, ListError> {
    let sql = format!("SELECT id FROM posts ORDER BY {sort} {}", if desc { "DESC" } else { "ASC" });
    sqlx::query_scalar(&sql).fetch_all(pool).await.map_err(ListError::Db)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    async fn order(db: &TestDb, sort: &str, desc: bool) -> Vec<i64> {
        match list_posts(&db.pool, sort, desc).await {
            Ok(ids) => ids,
            Err(ListError::BadSort) => panic!("ключ `{sort}` допустим, а функция его отвергла"),
            Err(ListError::Db(e)) => panic!("ошибка базы при сортировке по `{sort}`: {e}"),
        }
    }

    #[tokio::test]
    async fn allowed_keys_sort_correctly() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(order(&db, "id", true).await, vec![8, 7, 6, 5, 4, 3, 2, 1], "id по убыванию");
        assert_eq!(order(&db, "created_at", true).await, vec![6, 2, 5, 3, 8, 4, 1, 7], "сначала новые");
        assert_eq!(order(&db, "created_at", false).await, vec![7, 1, 4, 8, 3, 5, 2, 6], "сначала старые");
        assert_eq!(order(&db, "likes", true).await, vec![6, 1, 5, 8, 2, 3, 4, 7], "по лайкам вниз, при равенстве id по возрастанию");
        assert_eq!(order(&db, "likes", false).await, vec![3, 4, 7, 2, 5, 8, 1, 6], "по лайкам вверх, при равенстве id по возрастанию");
    }

    #[tokio::test]
    async fn everything_else_is_rejected_before_the_database() {
        let db = TestDb::with_sql(SEED).await;
        for bad in ["body", "karma", "likes_count", "", "id; DROP TABLE posts", "(SELECT 1)", "1"] {
            match list_posts(&db.pool, bad, false).await {
                Err(ListError::BadSort) => {}
                Ok(_) => panic!("ключ `{bad}` не из белого списка, но запрос выполнился"),
                Err(ListError::Db(e)) => panic!("ключ `{bad}` дошёл до базы (ошибка: {e}), а должен быть отвергнут раньше как BadSort"),
            }
        }
        let t = db.table("SELECT count(*) FROM posts").await;
        assert_eq!(t.cell(0, 0), "8", "таблица posts должна остаться на месте");
    }
}
