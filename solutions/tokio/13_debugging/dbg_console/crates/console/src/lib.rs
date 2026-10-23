use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use futures::FutureExt;
use futures::future::CatchUnwind;
use tokio::task::JoinHandle;
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Waiting,
    Finished,
    Panicked,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskInfo {
    pub id: u64,
    pub name: String,
    pub state: State,
    pub polls: u64,
    pub longest_poll: Duration,
    pub waiting_for: Option<Duration>,
}

type Hook = Arc<dyn Fn(&str, Duration) + Send + Sync>;

struct Record {
    id: u64,
    name: String,
    state: State,
    polls: u64,
    longest_poll: Duration,
    pending_since: Option<Instant>,
}

#[derive(Default)]
struct Registry {
    next_id: u64,
    records: Vec<Record>,
    long_poll: Option<(Duration, Hook)>,
}

#[derive(Clone, Default)]
pub struct Console {
    registry: Arc<Mutex<Registry>>,
}

impl Console {
    pub fn new() -> Console {
        Console::default()
    }

    pub fn spawn<F>(&self, name: &str, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let id = {
            let mut registry = self.registry.lock().unwrap();
            registry.next_id += 1;
            let id = registry.next_id;
            registry.records.push(Record {
                id,
                name: name.to_string(),
                state: State::Waiting,
                polls: 0,
                longest_poll: Duration::ZERO,
                pending_since: Some(Instant::now()),
            });
            id
        };
        let tracked = Tracked { id, console: self.clone(), inner: Box::pin(AssertUnwindSafe(future).catch_unwind()), done: false };
        tokio::spawn(async move {
            match tracked.await {
                Ok(value) => value,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        })
    }

    pub fn on_long_poll<H>(&self, threshold: Duration, hook: H)
    where
        H: Fn(&str, Duration) + Send + Sync + 'static,
    {
        self.registry.lock().unwrap().long_poll = Some((threshold, Arc::new(hook)));
    }

    pub fn tasks(&self) -> Vec<TaskInfo> {
        let now = Instant::now();
        let registry = self.registry.lock().unwrap();
        registry
            .records
            .iter()
            .map(|record| TaskInfo {
                id: record.id,
                name: record.name.clone(),
                state: record.state,
                polls: record.polls,
                longest_poll: record.longest_poll,
                waiting_for: match (record.state, record.pending_since) {
                    (State::Waiting, Some(since)) => Some(now - since),
                    _ => None,
                },
            })
            .collect()
    }

    pub fn alive(&self) -> usize {
        self.tasks().iter().filter(|task| matches!(task.state, State::Running | State::Waiting)).count()
    }

    pub fn stuck(&self, longer_than: Duration) -> Vec<String> {
        self.tasks().into_iter().filter(|task| task.waiting_for.is_some_and(|waited| waited > longer_than)).map(|task| task.name).collect()
    }

    pub fn prune(&self) -> usize {
        let mut registry = self.registry.lock().unwrap();
        let before = registry.records.len();
        registry.records.retain(|record| matches!(record.state, State::Running | State::Waiting));
        before - registry.records.len()
    }

    pub fn dump(&self) -> String {
        let tasks = self.tasks();
        let alive = tasks.iter().filter(|task| matches!(task.state, State::Running | State::Waiting)).count();
        let mut text = format!("tasks alive={alive} total={}", tasks.len());
        for task in tasks {
            let waiting = match task.waiting_for {
                Some(waited) => format!("{}ms", waited.as_millis()),
                None => "-".to_string(),
            };
            text.push_str(&format!("\n{} {} {:?} polls={} waiting={waiting}", task.id, task.name, task.state, task.polls));
        }
        text
    }

    fn set(&self, id: u64, update: impl FnOnce(&mut Record)) {
        let mut registry = self.registry.lock().unwrap();
        if let Some(record) = registry.records.iter_mut().find(|record| record.id == id) {
            update(record);
        }
    }
}

type Guarded<F> = CatchUnwind<AssertUnwindSafe<F>>;

struct Tracked<F: Future> {
    id: u64,
    console: Console,
    inner: Pin<Box<Guarded<F>>>,
    done: bool,
}

impl<F: Future> Future for Tracked<F> {
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let id = self.id;
        self.console.set(id, |record| {
            record.polls += 1;
            record.state = State::Running;
            record.pending_since = None;
        });
        let started = std::time::Instant::now();
        let out = self.inner.as_mut().poll(cx);
        let took = started.elapsed();
        let state = match &out {
            Poll::Pending => State::Waiting,
            Poll::Ready(Ok(_)) => State::Finished,
            Poll::Ready(Err(_)) => State::Panicked,
        };
        let report = {
            let mut registry = self.console.registry.lock().unwrap();
            let hook = registry.long_poll.clone();
            let mut name = String::new();
            if let Some(record) = registry.records.iter_mut().find(|record| record.id == id) {
                record.state = state;
                record.longest_poll = record.longest_poll.max(took);
                record.pending_since = (state == State::Waiting).then(Instant::now);
                name = record.name.clone();
            }
            hook.filter(|(threshold, _)| took > *threshold).map(|(_, hook)| (hook, name))
        };
        if let Some((hook, name)) = report {
            hook(&name, took);
        }
        if out.is_ready() {
            self.done = true;
        }
        out
    }
}

impl<F: Future> Drop for Tracked<F> {
    fn drop(&mut self) {
        if !self.done {
            let id = self.id;
            self.console.set(id, |record| record.state = State::Cancelled);
        }
    }
}
