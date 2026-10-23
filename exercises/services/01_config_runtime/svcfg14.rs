// Оркестратор видит состояние «запускаюсь» и терпеливо ждёт. Один из бэкендов недоступен
// навсегда (опечатка в адресе), и сервис не падает и не готовится: вечно «запускаюсь», пока
// человек не заглянет в логи. Цель: ожидание зависимостей ограничено общим временем `total`;
// по его истечении `Startup` переходит в `Failed` и называет тех, кого не дождался, чтобы
// оркестратор перезапустил сервис, а алерт показал причину.

use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{Instant, sleep};

pub struct Dependency {
    pub name: &'static str,
    pub probe: Box<dyn Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Starting,
    Running,
    Failed(String),
}

#[derive(Clone)]
pub struct Startup {
    state: Arc<Mutex<State>>,
}

impl Startup {
    pub fn new() -> Startup {
        Startup { state: Arc::new(Mutex::new(State::Starting)) }
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    pub async fn run(&self, deps: &[Dependency], interval: Duration, total: Duration) {
        let mut pending: Vec<&Dependency> = deps.iter().collect();
        loop {
            let mut still = Vec::new();
            for dep in pending {
                if (dep.probe)().await.is_err() {
                    still.push(dep);
                }
            }
            pending = still;
            if pending.is_empty() {
                *self.state.lock().unwrap() = State::Running;
                return;
            }
            sleep(interval).await;
        }
    }
}

impl Default for Startup {
    fn default() -> Self {
        Startup::new()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::timeout;

    fn dep_ready_after(name: &'static str, failures: u32) -> Dependency {
        let calls = Arc::new(AtomicU32::new(0));
        Dependency {
            name,
            probe: Box::new(move || {
                let calls = Arc::clone(&calls);
                Box::pin(async move {
                    if calls.fetch_add(1, Ordering::SeqCst) < failures { Err("refused".to_string()) } else { Ok(()) }
                })
            }),
        }
    }

    fn dep_never(name: &'static str) -> Dependency {
        Dependency { name, probe: Box::new(|| Box::pin(async { Err("refused".to_string()) })) }
    }

    const STEP: Duration = Duration::from_millis(100);

    #[tokio::test(start_paused = true)]
    async fn all_dependencies_up_means_running() {
        let startup = Startup::new();
        let deps = [dep_ready_after("db", 2), dep_ready_after("cache", 0)];
        timeout(Duration::from_secs(10), startup.run(&deps, STEP, Duration::from_secs(1))).await.unwrap();
        assert_eq!(startup.state(), State::Running, "все зависимости поднялись: Running");
    }

    #[tokio::test(start_paused = true)]
    async fn unreachable_dependency_fails_the_start() {
        let startup = Startup::new();
        let deps = [dep_ready_after("db", 1), dep_never("search")];
        let started = Instant::now();
        let run = timeout(Duration::from_secs(10), startup.run(&deps, STEP, Duration::from_secs(1))).await;
        assert!(run.is_ok(), "запуск завис: ожидание зависимостей не ограничено по времени");
        assert_eq!(
            startup.state(),
            State::Failed("search".to_string()),
            "в причине только те, кого не дождались (db поднялась)"
        );
        assert!(started.elapsed() <= Duration::from_secs(1), "дедлайн общий на всё: {:?}", started.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn state_is_starting_while_waiting() {
        let startup = Startup::new();
        let observer = startup.clone();
        let deps = [dep_never("db")];
        let task = tokio::spawn(async move { startup.run(&deps, STEP, Duration::from_secs(1)).await });
        sleep(Duration::from_millis(300)).await;
        assert_eq!(observer.state(), State::Starting, "пока ждём, состояние Starting");
        timeout(Duration::from_secs(10), task).await.expect("запуск не завершился").unwrap();
        assert_eq!(observer.state(), State::Failed("db".to_string()), "после дедлайна Failed");
    }

    #[tokio::test(start_paused = true)]
    async fn several_missing_dependencies_are_all_named() {
        let startup = Startup::new();
        let deps = [dep_never("db"), dep_never("cache")];
        timeout(Duration::from_secs(10), startup.run(&deps, STEP, Duration::from_millis(500))).await.unwrap();
        assert_eq!(startup.state(), State::Failed("db, cache".to_string()), "оба имени, через запятую, по порядку");
    }
}
