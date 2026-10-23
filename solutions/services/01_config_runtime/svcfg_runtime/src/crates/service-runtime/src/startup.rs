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
