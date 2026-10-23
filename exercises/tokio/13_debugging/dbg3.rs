// Конвейер из трёх шагов завис, и обёртки `Traced` уже расставлены по шагам и по самому конвейеру.
// Вместе они складываются в реестр. Нужно по реестру ответить: на каком шаге застряли, сколько
// опросов он пережил и как давно ждёт. Застрявшим считается самое внутреннее незавершённое ожидание:
// из реестра, где future лежат в порядке создания, это последнее незавершённое среди тех, что сейчас ждут.
// TODO: Напиши `diagnose`.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{Instant, sleep};

#[derive(Debug, Clone, Default)]
struct Stats {
    name: &'static str,
    polls: u32,
    pending_for: Duration,
    pending_since: Option<Instant>,
    finished: bool,
    task: Option<tokio::task::Id>,
}

struct Traced<F> {
    inner: Pin<Box<F>>,
    stats: Arc<Mutex<Stats>>,
}

impl<F: Future> Traced<F> {
    fn new(name: &'static str, inner: F) -> (Traced<F>, Arc<Mutex<Stats>>) {
        let stats = Arc::new(Mutex::new(Stats { name, ..Stats::default() }));
        (Traced { inner: Box::pin(inner), stats: stats.clone() }, stats)
    }
}

impl<F: Future> Future for Traced<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let now = Instant::now();
        {
            let mut stats = self.stats.lock().unwrap();
            stats.polls += 1;
            if stats.task.is_none() {
                stats.task = tokio::task::try_id();
            }
            if let Some(since) = stats.pending_since.take() {
                stats.pending_for += now - since;
            }
        }
        let out = self.inner.as_mut().poll(cx);
        let mut stats = self.stats.lock().unwrap();
        match out {
            Poll::Ready(_) => stats.finished = true,
            Poll::Pending => stats.pending_since = Some(Instant::now()),
        }
        out
    }
}

type Registry = Arc<Mutex<Vec<Arc<Mutex<Stats>>>>>;

fn traced<F: Future>(registry: &Registry, name: &'static str, inner: F) -> Traced<F> {
    let (wrapper, stats) = Traced::new(name, inner);
    registry.lock().unwrap().push(stats);
    wrapper
}

#[derive(Debug, PartialEq)]
struct Diagnosis {
    name: &'static str,
    polls: u32,
    waited: Duration,
}

async fn load() -> u32 {
    sleep(Duration::from_millis(20)).await;
    1
}

async fn convert(input: u32, stuck: bool) -> u32 {
    if stuck {
        std::future::pending::<()>().await;
    }
    sleep(Duration::from_millis(30)).await;
    input + 1
}

async fn store(input: u32, stuck: bool) -> u32 {
    if stuck {
        std::future::pending::<()>().await;
    }
    input * 10
}

async fn pipeline(registry: &Registry, stuck_at: &'static str) -> u32 {
    let loaded = traced(registry, "load", load()).await;
    let converted = traced(registry, "convert", convert(loaded, stuck_at == "convert")).await;
    traced(registry, "store", store(converted, stuck_at == "store")).await
}

fn diagnose(registry: &Registry) -> Option<Diagnosis> {
    let _ = registry;
    todo!()
}

#[tokio::main]
async fn main() {
    let registry: Registry = Arc::default();
    let whole = traced(&registry, "pipeline", pipeline(&registry, ""));
    println!("{}", whole.await);
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(stuck_at: &'static str, secs: u64) -> Registry {
        let registry: Registry = Arc::default();
        let task = {
            let registry = registry.clone();
            tokio::spawn(async move {
                let whole = traced(&registry, "pipeline", pipeline(&registry, stuck_at));
                whole.await
            })
        };
        sleep(Duration::from_secs(secs)).await;
        task.abort();
        let _ = task.await;
        registry
    }

    #[tokio::test(start_paused = true)]
    async fn names_the_step_that_never_woke() {
        let registry = run("convert", 1).await;
        assert_eq!(
            diagnose(&registry),
            Some(Diagnosis { name: "convert", polls: 1, waited: Duration::from_millis(980) }),
            "застрял convert: его опросили один раз, а ждёт он с 20-й миллисекунды до 1000-й"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn finds_a_stuck_last_step_too() {
        let registry = run("store", 1).await;
        assert_eq!(
            diagnose(&registry),
            Some(Diagnosis { name: "store", polls: 1, waited: Duration::from_millis(950) }),
            "застрял store: load и convert завершены, store ждёт с 50-й миллисекунды"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_stuck_in_a_finished_pipeline() {
        let registry = run("", 1).await;
        assert_eq!(diagnose(&registry), None, "конвейер дошёл до конца: диагностировать нечего");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_branch_is_not_blamed_when_it_has_finished() {
        let registry: Registry = Arc::default();
        let task = {
            let registry = registry.clone();
            tokio::spawn(async move {
                let left = traced(&registry, "left", sleep(Duration::from_millis(500)));
                let right = traced(&registry, "right", std::future::pending::<()>());
                tokio::join!(left, right);
            })
        };
        sleep(Duration::from_secs(1)).await;
        task.abort();
        let _ = task.await;
        let found = diagnose(&registry).expect("правая ветка не закончилась");
        assert_eq!(found.name, "right", "левая ветка закончилась через 500 мс: виновата правая");
        assert_eq!(found.polls, 2, "join опросил правую ветку дважды: при старте и когда проснулась левая");
        assert_eq!(found.waited, Duration::from_millis(500), "ожидание считается от последнего Pending: правая ветка получила его на 500-й миллисекунде");
    }

    #[tokio::test(start_paused = true)]
    async fn the_innermost_wait_wins_over_the_outer_one() {
        let registry = run("convert", 1).await;
        let outer_unfinished = registry.lock().unwrap().iter().any(|stats| {
            let stats = stats.lock().unwrap();
            stats.name == "pipeline" && !stats.finished
        });
        assert!(outer_unfinished, "сам конвейер тоже не закончился, но виноват не он");
        assert_ne!(diagnose(&registry).unwrap().name, "pipeline", "внешняя обёртка не виновата: ждёт внутренний шаг");
    }
}
