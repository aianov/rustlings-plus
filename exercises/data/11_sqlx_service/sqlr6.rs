// Сервис берёт настройки пула из конфига. Сейчас `build_pool` игнорирует их и открывает пул
// как попало: десять соединений, ожидание 30 секунд, и никаких пометок. Нужно:
// предел соединений, время ожидания свободного, время простоя, а каждое соединение пула
// (не только первое!) при открытии получает имя приложения для pg_stat_activity и свой
// statement_timeout.
// TODO: Допиши сборку пула. Подсказка про `SET`: он не принимает параметры, зато есть `set_config`.

use sqlx::PgPool;

struct PoolConfig {
    url: String,
    max_connections: u32,
    acquire_timeout_ms: u64,
    idle_timeout_s: u64,
    app_name: String,
    statement_timeout_ms: u64,
}

async fn build_pool(cfg: &PoolConfig) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(&cfg.url).await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn config() -> PoolConfig {
        PoolConfig {
            url: course_support::pg::url(),
            max_connections: 2,
            acquire_timeout_ms: 300,
            idle_timeout_s: 45,
            app_name: "rustlings-sqlr6".to_string(),
            statement_timeout_ms: 5000,
        }
    }

    #[tokio::test]
    async fn pool_waits_only_as_configured() {
        let pool = build_pool(&config()).await.expect("пул должен открыться");
        let _a = pool.acquire().await.expect("первое соединение");
        let _b = pool.acquire().await.expect("второе соединение");
        let third = tokio::time::timeout(Duration::from_secs(5), pool.acquire()).await.expect(
            "третье соединение ждёт слишком долго: acquire_timeout из конфига (300 мс) не применён, либо предел соединений больше двух",
        );
        assert!(matches!(third, Err(sqlx::Error::PoolTimedOut)), "при занятых двух соединениях третье должно получить PoolTimedOut");
        assert_eq!(pool.options().get_idle_timeout(), Some(Duration::from_secs(45)), "idle_timeout из конфига не применён");
    }

    #[tokio::test]
    async fn every_connection_gets_the_settings() {
        let pool = build_pool(&config()).await.expect("пул должен открыться");
        let mut a = pool.acquire().await.unwrap();
        let mut b = pool.acquire().await.unwrap();
        for (n, conn) in [&mut a, &mut b].into_iter().enumerate() {
            let app: String = sqlx::query_scalar("SHOW application_name").fetch_one(&mut **conn).await.unwrap();
            assert_eq!(app, "rustlings-sqlr6", "соединение {n}: application_name не выставлен (after_connect должен работать для каждого нового соединения)");
            let stmt: String = sqlx::query_scalar("SHOW statement_timeout").fetch_one(&mut **conn).await.unwrap();
            assert_eq!(stmt, "5s", "соединение {n}: statement_timeout должен быть 5 секунд");
        }
    }

    #[tokio::test]
    async fn closed_pool_refuses_work() {
        let pool = build_pool(&config()).await.expect("пул должен открыться");
        pool.close().await;
        assert!(sqlx::query("SELECT 1").execute(&pool).await.is_err(), "после close пул не должен выдавать соединения");
    }
}
