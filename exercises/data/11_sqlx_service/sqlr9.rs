// Поиск по постам иногда упирается в тяжёлый запрос, и HTTP-обработчик ждёт ответа вечно.
// `slow_posts` ниже имитирует такой запрос (`pause_s` секунд работы базы). Нужна обёртка
// `posts_within`: если ответа нет за `deadline`, она не ждёт дальше и возвращает `TooSlow`.
// Ошибки базы остаются ошибками базы. Какая судьба у самого запроса на сервере, разбери по README.
// TODO: Допиши обёртку.

use sqlx::PgPool;
use std::time::Duration;

#[derive(Debug)]
enum SearchError {
    TooSlow,
    Db(sqlx::Error),
}

async fn slow_posts(pool: &PgPool, pause_s: f64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM posts, pg_sleep($1) ORDER BY id")
        .bind(pause_s)
        .fetch_all(pool)
        .await
}

async fn posts_within(pool: &PgPool, pause_s: f64, deadline: Duration) -> Result<Vec<i64>, SearchError> {
    slow_posts(pool, pause_s).await.map_err(SearchError::Db)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::pg::TestDb;

    #[tokio::test]
    async fn fast_query_returns_rows() {
        let db = TestDb::with_sql(include_str!("seed.sql")).await;
        let ids = posts_within(&db.pool, 0.0, Duration::from_secs(5)).await;
        match ids {
            Ok(ids) => assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 7, 8], "быстрый запрос должен вернуть все восемь постов по порядку"),
            Err(SearchError::TooSlow) => panic!("запрос без паузы успевает за 5 секунд, а функция сказала TooSlow"),
            Err(SearchError::Db(e)) => panic!("неожиданная ошибка базы: {e}"),
        }
    }

    #[tokio::test]
    async fn slow_query_is_abandoned_at_the_deadline() {
        let db = TestDb::with_sql(include_str!("seed.sql")).await;
        let started = std::time::Instant::now();
        let res = tokio::time::timeout(Duration::from_millis(1500), posts_within(&db.pool, 2.0, Duration::from_millis(200)))
            .await
            .expect("функция ждёт ответа базы дольше срока: deadline не применён");
        assert!(matches!(res, Err(SearchError::TooSlow)), "запрос на 2 секунды при сроке 200 мс должен закончиться TooSlow");
        assert!(started.elapsed() < Duration::from_millis(1500), "ответ TooSlow пришёл слишком поздно");
        let again = posts_within(&db.pool, 0.0, Duration::from_secs(5)).await;
        assert!(matches!(again, Ok(ref ids) if ids.len() == 8), "после отказа от долгого запроса пул должен продолжать работать");
    }
}
