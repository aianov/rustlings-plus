// Статистика просмотров. Две функции.
//  - `views_summary`: одна строка по всем постам со столбцами total (сумма просмотров), average
//    (среднее, округлённое до одного знака функцией round), smallest и largest. У некоторых постов просмотры
//    ещё не посчитаны (NULL), это «неизвестно», а не ноль.
//  - `views_of_author`: $1 это id автора, результат одно число, сумма просмотров его постов.
//    У автора без постов или без посчитанных просмотров должен получиться 0, а не NULL.
// TODO: Напиши оба запроса.

fn views_summary() -> &'static str {
    "SELECT 1"
}

fn views_of_author() -> &'static str {
    "SELECT sum(views) FROM posts WHERE author_id = $1"
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn summary_ignores_unknown_views() {
        let db = TestDb::with_sql(SEED).await;
        let t = db.table(views_summary()).await;
        assert_eq!(t.columns, ["total", "average", "smallest", "largest"], "нужны столбцы total, average, smallest, largest");
        assert_eq!(t.rows, [["610", "101.7", "30", "250"]], "просмотры известны у шести постов из восьми, среднее считается по ним (610 / 6), а не по всем:\n{t}");
    }

    async fn views_of(db: &TestDb, author: i64) -> i64 {
        sqlx::query_scalar::<_, i64>(views_of_author())
            .bind(author)
            .fetch_one(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("views_of_author({author}): {e}. Сумма по пустому набору это NULL, а нужен 0"))
    }

    #[tokio::test]
    async fn empty_sum_is_zero() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(views_of(&db, 3).await, 30, "у carol просмотры известны только у одного поста");
        assert_eq!(views_of(&db, 4).await, 0, "у dave единственный пост без посчитанных просмотров");
        assert_eq!(views_of(&db, 5).await, 0, "у eve постов нет вовсе");
    }
}
