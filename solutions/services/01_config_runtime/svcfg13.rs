use std::future::Future;
use std::time::Duration;
use tokio::time::{Instant, sleep};

pub struct Policy {
    pub interval: Duration,
    pub total: Duration,
}

#[derive(Debug, PartialEq)]
pub enum StartupError {
    Config(String),
    Dependency { name: String, attempts: u32, last_error: String },
}

pub async fn wait_for<F, Fut>(name: &str, policy: &Policy, mut probe: F) -> Result<u32, StartupError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let started = Instant::now();
    let mut attempts = 0;
    loop {
        attempts += 1;
        match probe().await {
            Ok(()) => return Ok(attempts),
            Err(last_error) => {
                if started.elapsed() + policy.interval > policy.total {
                    return Err(StartupError::Dependency { name: name.to_string(), attempts, last_error });
                }
                sleep(policy.interval).await;
            }
        }
    }
}

pub async fn start<F, Fut>(
    source: impl Fn(&str) -> Option<String>,
    policy: &Policy,
    mut probe: F,
) -> Result<u32, StartupError>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let url = source("DATABASE_URL")
        .ok_or_else(|| StartupError::Config("переменная DATABASE_URL не задана".to_string()))?;
    wait_for("database", policy, || probe(url.clone())).await
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const POLICY: Policy = Policy { interval: Duration::from_millis(100), total: Duration::from_millis(350) };

    #[tokio::test(start_paused = true)]
    async fn ready_dependency_needs_one_attempt() {
        let attempts = wait_for("db", &POLICY, || async { Ok(()) }).await;
        assert_eq!(attempts, Ok(1), "зависимость сразу готова: одна попытка и без пауз");
    }

    #[tokio::test(start_paused = true)]
    async fn dependency_that_comes_up_later_is_awaited() {
        let calls = AtomicU32::new(0);
        let started = Instant::now();
        let attempts = wait_for("db", &POLICY, || async {
            if calls.fetch_add(1, Ordering::SeqCst) < 2 { Err("not yet".to_string()) } else { Ok(()) }
        })
        .await;
        assert_eq!(attempts, Ok(3), "два отказа и успех: три попытки");
        assert_eq!(started.elapsed(), Duration::from_millis(200), "между попытками пауза interval");
    }

    #[tokio::test(start_paused = true)]
    async fn giving_up_names_the_dependency_and_the_last_error() {
        let started = Instant::now();
        let err = wait_for("db", &POLICY, || async { Err("connection refused".to_string()) })
            .await
            .unwrap_err();
        assert_eq!(
            err,
            StartupError::Dependency { name: "db".to_string(), attempts: 4, last_error: "connection refused".to_string() },
            "попытки в 0, 100, 200 и 300 мс: четыре; потом следующая выходила бы за дедлайн 350 мс"
        );
        assert!(started.elapsed() <= POLICY.total, "ждать дольше общего дедлайна нельзя: {:?}", started.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn bad_config_fails_without_a_single_probe() {
        let probes = AtomicU32::new(0);
        let err = start(|_| None, &POLICY, |_url| async {
            probes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap_err();
        assert!(matches!(err, StartupError::Config(_)), "нет DATABASE_URL: ошибка конфига, а было {err:?}");
        assert_eq!(probes.load(Ordering::SeqCst), 0, "кривой конфиг ожиданием не лечится: ни одной попытки");
    }

    #[tokio::test(start_paused = true)]
    async fn good_config_probes_the_given_url() {
        let seen = std::sync::Mutex::new(Vec::new());
        let attempts = start(
            |name| (name == "DATABASE_URL").then(|| "postgres://db".to_string()),
            &POLICY,
            |url| {
                seen.lock().unwrap().push(url);
                async { Ok(()) }
            },
        )
        .await;
        assert_eq!(attempts, Ok(1), "база ответила сразу");
        assert_eq!(*seen.lock().unwrap(), vec!["postgres://db".to_string()], "проба получает адрес из конфига");
    }
}
