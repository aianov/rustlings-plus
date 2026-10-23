// Профиль автора показывает два числа: сколько у него постов и сколько лайков всего. Страница
// профиля падает с ошибкой: у всех авторов с одной, а у авторов без постов ещё и с другой.
// Сами SQL-запросы написаны верно. Починить нужно код вокруг них, не меняя сигнатуры функций.
// TODO: Найди обе причины (их две) и исправь.

use sqlx::PgPool;

async fn posts_count(pool: &PgPool, author_id: i64) -> Result<i64, sqlx::Error> {
    let n: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE author_id = $1")
        .bind(author_id)
        .fetch_one(pool)
        .await?;
    Ok(i64::from(n))
}

async fn likes_total(pool: &PgPool, author_id: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT SUM(likes) FROM posts WHERE author_id = $1")
        .bind(author_id)
        .fetch_one(pool)
        .await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    const SEED: &str = include_str!("seed.sql");

    #[tokio::test]
    async fn counts_posts() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(posts_count(&db.pool, 1).await.expect("COUNT(*) должен читаться без ошибки типов"), 3, "у alice три поста");
        assert_eq!(posts_count(&db.pool, 4).await.expect("у dave постов нет, но это число 0"), 0, "у dave нет постов");
    }

    #[tokio::test]
    async fn sums_likes() {
        let db = TestDb::with_sql(SEED).await;
        assert_eq!(likes_total(&db.pool, 1).await.expect("сумма должна прочитаться"), 39, "лайки alice: 10 + 4 + 25");
        assert_eq!(likes_total(&db.pool, 3).await.unwrap(), 3, "лайки carol: 0 + 3");
    }

    #[tokio::test]
    async fn author_without_posts_has_zero_likes() {
        let db = TestDb::with_sql(SEED).await;
        let got = likes_total(&db.pool, 6).await;
        assert_eq!(got.expect("SUM по пустому набору это NULL, а функция обещает число: для автора без постов ждали 0"), 0, "у frank нет постов, лайков 0");
    }
}
