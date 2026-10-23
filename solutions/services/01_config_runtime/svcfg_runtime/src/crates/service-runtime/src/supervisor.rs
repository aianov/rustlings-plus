use futures::FutureExt;
use futures::future::BoxFuture;
use std::panic::AssertUnwindSafe;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio::time::sleep;

pub type Factory = Box<dyn Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

pub struct TaskSpec {
    pub name: &'static str,
    pub critical: bool,
    pub factory: Factory,
}

#[derive(Clone, Copy)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub pause: Duration,
}

#[derive(Debug, PartialEq)]
pub enum SupervisorError {
    CriticalFailed { name: &'static str, last_error: String },
}

struct GaveUp {
    name: &'static str,
    critical: bool,
    last_error: String,
}

async fn supervise(spec: TaskSpec, policy: RestartPolicy) -> Option<GaveUp> {
    let mut restarts = 0;
    loop {
        let last_error = match AssertUnwindSafe((spec.factory)()).catch_unwind().await {
            Ok(Ok(())) => return None,
            Ok(Err(error)) => error,
            Err(_) => "паника".to_string(),
        };
        if restarts >= policy.max_restarts {
            return Some(GaveUp { name: spec.name, critical: spec.critical, last_error });
        }
        restarts += 1;
        sleep(policy.pause).await;
    }
}

pub async fn run(
    specs: Vec<TaskSpec>,
    policy: RestartPolicy,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), SupervisorError> {
    let mut tasks = JoinSet::new();
    for spec in specs {
        tasks.spawn(supervise(spec, policy));
    }
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                tasks.abort_all();
                return Ok(());
            }
            next = tasks.join_next() => match next {
                None => return Ok(()),
                Some(Ok(Some(gave_up))) if gave_up.critical => {
                    tasks.abort_all();
                    return Err(SupervisorError::CriticalFailed {
                        name: gave_up.name,
                        last_error: gave_up.last_error,
                    });
                }
                Some(_) => {}
            }
        }
    }
}
