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
pub struct Policy {
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

async fn supervise(spec: TaskSpec, policy: Policy) -> Option<GaveUp> {
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
    policy: Policy,
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use tokio::time::timeout;

    const POLICY: Policy = Policy { max_restarts: 2, pause: Duration::from_millis(50) };

    fn failing_times(name: &'static str, critical: bool, failures: u32, panic: bool, runs: &Arc<AtomicU32>) -> TaskSpec {
        let runs = Arc::clone(runs);
        TaskSpec {
            name,
            critical,
            factory: Box::new(move || {
                let runs = Arc::clone(&runs);
                Box::pin(async move {
                    let n = runs.fetch_add(1, Ordering::SeqCst);
                    if n < failures {
                        if panic {
                            panic!("сбой задачи");
                        }
                        return Err(format!("сбой {n}"));
                    }
                    Ok(())
                })
            }),
        }
    }

    fn forever(name: &'static str, ticks: &Arc<AtomicUsize>) -> TaskSpec {
        let ticks = Arc::clone(ticks);
        TaskSpec {
            name,
            critical: false,
            factory: Box::new(move || {
                let ticks = Arc::clone(&ticks);
                Box::pin(async move {
                    loop {
                        ticks.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(10)).await;
                    }
                })
            }),
        }
    }

    fn channel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
        watch::channel(false)
    }

    #[tokio::test(start_paused = true)]
    async fn flaky_task_is_restarted_until_it_succeeds() {
        let runs = Arc::new(AtomicU32::new(0));
        let (_tx, rx) = channel();
        let result = run(vec![failing_times("cleanup", false, 2, false, &runs)], POLICY, rx).await;
        assert_eq!(result, Ok(()), "задача отработала, супервизор завершился спокойно");
        assert_eq!(runs.load(Ordering::SeqCst), 3, "два сбоя и успех: три запуска");
    }

    #[tokio::test(start_paused = true)]
    async fn panic_counts_as_a_failure() {
        let runs = Arc::new(AtomicU32::new(0));
        let (_tx, rx) = channel();
        let result = run(vec![failing_times("feed", false, 1, true, &runs)], POLICY, rx).await;
        assert_eq!(result, Ok(()), "паника не должна ронять супервизор");
        assert_eq!(runs.load(Ordering::SeqCst), 2, "после паники задача запущена снова");
    }

    #[tokio::test(start_paused = true)]
    async fn non_critical_task_may_give_up_quietly() {
        let broken = Arc::new(AtomicU32::new(0));
        let healthy = Arc::new(AtomicU32::new(0));
        let slow_ok = TaskSpec {
            name: "slow",
            critical: false,
            factory: {
                let healthy = Arc::clone(&healthy);
                Box::new(move || {
                    let healthy = Arc::clone(&healthy);
                    Box::pin(async move {
                        sleep(Duration::from_secs(1)).await;
                        healthy.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                })
            },
        };
        let (_tx, rx) = channel();
        let result = run(vec![failing_times("mailer", false, 100, false, &broken), slow_ok], POLICY, rx).await;
        assert_eq!(result, Ok(()), "отказ некритичной задачи не роняет процесс");
        assert_eq!(broken.load(Ordering::SeqCst), 3, "первый запуск и два перезапуска, потом сдаёмся");
        assert_eq!(healthy.load(Ordering::SeqCst), 1, "остальные задачи доработали до конца");
    }

    #[tokio::test(start_paused = true)]
    async fn critical_failure_stops_everything() {
        let runs = Arc::new(AtomicU32::new(0));
        let ticks = Arc::new(AtomicUsize::new(0));
        let (_tx, rx) = channel();
        let result = timeout(
            Duration::from_secs(10),
            run(vec![failing_times("indexer", true, 100, false, &runs), forever("ticker", &ticks)], POLICY, rx),
        )
        .await
        .expect("критичный отказ должен завершать run, а он завис");
        assert_eq!(
            result,
            Err(SupervisorError::CriticalFailed { name: "indexer", last_error: "сбой 2".to_string() }),
            "ошибка называет критичную задачу и её последнюю ошибку"
        );
        let after = ticks.load(Ordering::SeqCst);
        sleep(Duration::from_millis(200)).await;
        assert_eq!(ticks.load(Ordering::SeqCst), after, "остальные задачи должны быть остановлены, а не брошены работать");
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_signal_stops_all_tasks() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        let handle = tokio::spawn(run(vec![forever("ticker", &ticks)], POLICY, rx));
        sleep(Duration::from_millis(100)).await;
        tx.send(true).unwrap();
        let result = timeout(Duration::from_secs(5), handle).await.expect("run не вернулся после остановки").unwrap();
        assert_eq!(result, Ok(()), "остановка по команде это нормальный выход");
        let after = ticks.load(Ordering::SeqCst);
        sleep(Duration::from_millis(100)).await;
        assert_eq!(ticks.load(Ordering::SeqCst), after, "после остановки задачи не работают");
    }
}
