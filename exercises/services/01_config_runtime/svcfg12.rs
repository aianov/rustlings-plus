// Стоит упасть базе, как все экземпляры сервиса перезапускаются по кругу, а при каждой
// выкатке пользователи видят обрывы запросов: оркестратор убивает процесс в разгар остановки.
// Оркестратор дёргает `/health` и убивает процесс, если ответ плохой. Тесты ниже описывают, что
// сервис обязан отвечать в трёх ситуациях: зависимость упала, зависимость зависла, идёт остановка.
// Цель: оркестратор убивает только по-настоящему зависший процесс, а трафик гасит `/ready`.

use futures::future::{BoxFuture, join_all};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub trait Check: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self) -> BoxFuture<'_, Result<(), String>>;
}

#[derive(Debug, PartialEq)]
pub struct Report {
    pub ready: bool,
    pub failures: Vec<(String, String)>,
}

pub struct Health {
    checks: Vec<Arc<dyn Check>>,
    shutting_down: AtomicBool,
    check_timeout: Duration,
}

impl Health {
    pub fn new(check_timeout: Duration) -> Health {
        Health { checks: Vec::new(), shutting_down: AtomicBool::new(false), check_timeout }
    }

    pub fn register(&mut self, check: Arc<dyn Check>) {
        self.checks.push(check);
    }

    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn live(&self) -> bool {
        !self.shutting_down.load(Ordering::SeqCst)
    }

    pub async fn ready(&self) -> Report {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Report {
                ready: false,
                failures: vec![("shutdown".to_string(), "идёт остановка".to_string())],
            };
        }
        let results = join_all(self.checks.iter().map(|check| async move {
            match tokio::time::timeout(self.check_timeout, check.run()).await {
                Ok(Ok(())) => None,
                Ok(Err(reason)) => Some((check.name().to_string(), reason)),
                Err(_) => Some((check.name().to_string(), "timeout".to_string())),
            }
        }))
        .await;
        let failures: Vec<_> = results.into_iter().flatten().collect();
        Report { ready: failures.is_empty(), failures }
    }
}

pub async fn route(health: &Health, path: &str) -> u16 {
    match path {
        "/health" => {
            if health.ready().await.ready {
                200
            } else {
                503
            }
        }
        "/ready" => {
            if health.ready().await.ready {
                200
            } else {
                503
            }
        }
        _ => 404,
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    struct Down;
    struct Hangs;

    impl Check for Down {
        fn name(&self) -> &str {
            "db"
        }

        fn run(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async { Err("connection refused".to_string()) })
        }
    }

    impl Check for Hangs {
        fn name(&self) -> &str {
            "cache"
        }

        fn run(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(std::future::pending())
        }
    }

    fn broken_health() -> Health {
        let mut health = Health::new(Duration::from_secs(1));
        health.register(Arc::new(Down));
        health
    }

    #[tokio::test(start_paused = true)]
    async fn liveness_survives_a_dead_dependency() {
        let health = broken_health();
        assert_eq!(route(&health, "/health").await, 200, "база упала, но процесс жив: /health обязан отвечать 200");
        assert_eq!(route(&health, "/ready").await, 503, "а вот /ready должен сказать «не готов»");
    }

    #[tokio::test(start_paused = true)]
    async fn liveness_does_not_wait_for_dependencies() {
        let mut health = Health::new(Duration::from_secs(30));
        health.register(Arc::new(Hangs));
        let answer = timeout(Duration::from_millis(100), route(&health, "/health")).await;
        assert_eq!(answer, Ok(200), "/health не должен ждать зависшую зависимость: он вообще не ходит к проверкам");
    }

    #[tokio::test(start_paused = true)]
    async fn liveness_stays_up_during_shutdown() {
        let health = broken_health();
        health.begin_shutdown();
        assert_eq!(route(&health, "/health").await, 200, "при остановке процесс жив: убивать его по liveness рано");
        assert_eq!(route(&health, "/ready").await, 503, "при остановке /ready отвечает «не готов»");
    }

    #[tokio::test(start_paused = true)]
    async fn healthy_service_and_unknown_paths() {
        let health = Health::new(Duration::from_secs(1));
        assert_eq!(route(&health, "/ready").await, 200, "без проверок сервис готов");
        assert_eq!(route(&health, "/nope").await, 404, "неизвестный путь");
    }
}
