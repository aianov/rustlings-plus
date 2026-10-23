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
        true
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, sleep};

    enum Behaviour {
        Ok,
        Fail(&'static str),
        Slow(Duration),
        Hang,
    }

    struct Fake {
        name: &'static str,
        behaviour: Behaviour,
    }

    impl Check for Fake {
        fn name(&self) -> &str {
            self.name
        }

        fn run(&self) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async move {
                match &self.behaviour {
                    Behaviour::Ok => Ok(()),
                    Behaviour::Fail(why) => Err(why.to_string()),
                    Behaviour::Slow(d) => {
                        sleep(*d).await;
                        Ok(())
                    }
                    Behaviour::Hang => std::future::pending().await,
                }
            })
        }
    }

    fn health(checks: Vec<(&'static str, Behaviour)>) -> Health {
        let mut health = Health::new(Duration::from_secs(1));
        for (name, behaviour) in checks {
            health.register(Arc::new(Fake { name, behaviour }));
        }
        health
    }

    #[tokio::test(start_paused = true)]
    async fn all_checks_pass() {
        let report = health(vec![("db", Behaviour::Ok), ("cache", Behaviour::Ok)]).ready().await;
        assert_eq!(report, Report { ready: true, failures: vec![] }, "все проверки прошли: сервис готов");
    }

    #[tokio::test(start_paused = true)]
    async fn failed_check_is_named_with_its_reason() {
        let report = health(vec![("db", Behaviour::Fail("connection refused")), ("cache", Behaviour::Ok)])
            .ready()
            .await;
        assert!(!report.ready, "база не отвечает: сервис не готов");
        assert_eq!(
            report.failures,
            vec![("db".to_string(), "connection refused".to_string())],
            "в отчёте имя проверки и причина"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn hanging_check_times_out() {
        let started = Instant::now();
        let report = health(vec![("db", Behaviour::Hang)]).ready().await;
        assert_eq!(
            report.failures,
            vec![("db".to_string(), "timeout".to_string())],
            "зависшая проверка это провал с причиной timeout"
        );
        assert!(started.elapsed() <= Duration::from_millis(1100), "ready не должен ждать дольше лимита проверки");
    }

    #[tokio::test(start_paused = true)]
    async fn checks_run_concurrently() {
        let started = Instant::now();
        let report = health(vec![
            ("a", Behaviour::Slow(Duration::from_millis(400))),
            ("b", Behaviour::Slow(Duration::from_millis(400))),
            ("c", Behaviour::Slow(Duration::from_millis(400))),
        ])
        .ready()
        .await;
        assert!(report.ready, "медленные, но успешные проверки");
        assert!(
            started.elapsed() < Duration::from_millis(600),
            "три проверки по 400 мс подряд это 1200 мс, а одновременно около 400: прошло {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn live_does_not_depend_on_dependencies_or_shutdown() {
        let health = health(vec![("db", Behaviour::Fail("down"))]);
        assert!(health.live(), "зависимость упала, но процесс жив");
        health.begin_shutdown();
        assert!(health.live(), "во время остановки процесс тоже ещё жив");
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_makes_the_service_not_ready() {
        let health = health(vec![("db", Behaviour::Ok)]);
        health.begin_shutdown();
        assert!(!health.ready().await.ready, "идёт остановка: готовности нет, даже если зависимости в порядке");
    }
}
