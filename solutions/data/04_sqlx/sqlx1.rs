use sqlx::{PgPool, Row};

fn pg_url() -> String {
    std::env::var("RUSTLINGS_PG_URL")
        .unwrap_or_else(|_| "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings".to_string())
}

async fn connect_to(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(url).await
}

async fn connect() -> Result<PgPool, sqlx::Error> {
    connect_to(&pg_url()).await
}

async fn answer(pool: &PgPool) -> Result<i32, sqlx::Error> {
    let row = sqlx::query("SELECT 41 + 1 AS answer").fetch_one(pool).await?;
    Ok(row.get("answer"))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_comes_from_env_or_default() {
        assert_eq!(pg_url(), course_support::pg::url(), "адрес должен браться из RUSTLINGS_PG_URL, а без неё быть адресом курса");
    }

    #[tokio::test]
    async fn pool_answers() {
        let pool = connect().await.expect("пул должен подключиться к Postgres курса");
        assert_eq!(answer(&pool).await.expect("запрос должен выполниться"), 42, "SELECT 41 + 1 AS answer должен вернуть 42");
    }

    #[tokio::test]
    async fn pool_serves_many_tasks_at_once() {
        let pool = connect().await.expect("пул должен подключиться");
        let mut handles = Vec::new();
        for _ in 0..20 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move { answer(&pool).await }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap().expect("запрос в задаче"), 42, "пул можно клонировать и ходить в него из многих задач");
        }
    }

    #[tokio::test]
    async fn bad_url_is_an_error_not_a_panic() {
        let res = connect_to("postgres://nobody:wrong@127.0.0.1:55432/none").await;
        assert!(res.is_err(), "с неверным паролем подключение должно вернуть Err, а не пул");
    }
}
