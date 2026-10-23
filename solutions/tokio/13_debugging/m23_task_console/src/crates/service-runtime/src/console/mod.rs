mod traced;

use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Write};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::Instant;

pub use traced::Traced;

pub const DEFAULT_LONG_POLL: Duration = Duration::from_millis(100);

pub const KEPT_LONG_POLLS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Idle,
    Polling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInfo {
    pub id: u64,
    pub name: &'static str,
    pub state: TaskState,
    pub polls: u64,
    pub since: Duration,
    pub longest_poll: Duration,
}

impl fmt::Display for TaskInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let since = self.since.as_millis();
        match self.state {
            TaskState::Idle => write!(f, "#{} {}: ждёт {since} мс", self.id, self.name)?,
            TaskState::Polling => write!(f, "#{} {}: в poll {since} мс", self.id, self.name)?,
        }
        write!(f, ", poll {}", self.polls)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskCounts {
    pub alive: usize,
    pub spawned: u64,
    pub finished: u64,
    pub cancelled: u64,
    pub panicked: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LongPoll {
    pub id: u64,
    pub name: &'static str,
    pub took: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ended {
    Finished,
    Cancelled,
    Panicked,
}

#[derive(Debug)]
struct Progress {
    polls: u64,
    polling: bool,
    since: Instant,
    longest: Duration,
    panicked: bool,
}

#[derive(Debug)]
struct Slot {
    id: u64,
    name: &'static str,
    progress: Mutex<Progress>,
}

impl Slot {
    fn progress(&self) -> MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn info(&self, now: Instant) -> TaskInfo {
        let progress = self.progress();
        TaskInfo {
            id: self.id,
            name: self.name,
            state: if progress.polling {
                TaskState::Polling
            } else {
                TaskState::Idle
            },
            polls: progress.polls,
            since: now.saturating_duration_since(progress.since),
            longest_poll: progress.longest,
        }
    }
}

struct Registry {
    next: AtomicU64,
    live: Mutex<BTreeMap<u64, Arc<Slot>>>,
    long: Mutex<VecDeque<LongPoll>>,
    threshold: Duration,
    spawned: AtomicU64,
    finished: AtomicU64,
    cancelled: AtomicU64,
    panicked: AtomicU64,
}

impl Registry {
    fn live(&self) -> MutexGuard<'_, BTreeMap<u64, Arc<Slot>>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn long(&self) -> MutexGuard<'_, VecDeque<LongPoll>> {
        self.long.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn register(&self, name: &'static str) -> Arc<Slot> {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let slot = Arc::new(Slot {
            id,
            name,
            progress: Mutex::new(Progress {
                polls: 0,
                polling: false,
                since: Instant::now(),
                longest: Duration::ZERO,
                panicked: false,
            }),
        });
        self.spawned.fetch_add(1, Ordering::Relaxed);
        self.live().insert(id, Arc::clone(&slot));
        slot
    }

    fn slow(&self, slot: &Slot, took: Duration) {
        if took <= self.threshold {
            return;
        }
        let mut long = self.long();
        if long.len() == KEPT_LONG_POLLS {
            long.pop_front();
        }
        long.push_back(LongPoll {
            id: slot.id,
            name: slot.name,
            took,
        });
    }

    fn end(&self, id: u64, ended: Ended) {
        self.live().remove(&id);
        let counter = match ended {
            Ended::Finished => &self.finished,
            Ended::Cancelled => &self.cancelled,
            Ended::Panicked => &self.panicked,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub struct Console {
    registry: Arc<Registry>,
}

impl Default for Console {
    fn default() -> Console {
        Console::with_threshold(DEFAULT_LONG_POLL)
    }
}

impl Console {
    pub fn new() -> Console {
        Console::default()
    }

    pub fn with_threshold(threshold: Duration) -> Console {
        Console {
            registry: Arc::new(Registry {
                next: AtomicU64::new(0),
                live: Mutex::new(BTreeMap::new()),
                long: Mutex::new(VecDeque::with_capacity(KEPT_LONG_POLLS)),
                threshold,
                spawned: AtomicU64::new(0),
                finished: AtomicU64::new(0),
                cancelled: AtomicU64::new(0),
                panicked: AtomicU64::new(0),
            }),
        }
    }

    pub fn threshold(&self) -> Duration {
        self.registry.threshold
    }

    pub fn traced<F: Future>(&self, name: &'static str, future: F) -> Traced<F> {
        let slot = self.registry.register(name);
        Traced::new(future, slot, Arc::clone(&self.registry))
    }

    pub fn spawn<F>(&self, name: &'static str, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(self.traced(name, future))
    }

    pub fn tasks(&self) -> Vec<TaskInfo> {
        let now = Instant::now();
        let slots: Vec<Arc<Slot>> = self.registry.live().values().cloned().collect();
        slots.iter().map(|slot| slot.info(now)).collect()
    }

    pub fn alive(&self) -> usize {
        self.registry.live().len()
    }

    pub fn counts(&self) -> TaskCounts {
        let registry = &self.registry;
        TaskCounts {
            alive: self.alive(),
            spawned: registry.spawned.load(Ordering::Relaxed),
            finished: registry.finished.load(Ordering::Relaxed),
            cancelled: registry.cancelled.load(Ordering::Relaxed),
            panicked: registry.panicked.load(Ordering::Relaxed),
        }
    }

    pub fn long_polls(&self) -> Vec<LongPoll> {
        self.registry.long().iter().copied().collect()
    }

    pub fn dump(&self) -> String {
        let counts = self.counts();
        let mut text = format!(
            "задач живых {}, запущено {}, завершено {}, брошено {}, паник {}",
            counts.alive, counts.spawned, counts.finished, counts.cancelled, counts.panicked
        );
        for task in self.tasks() {
            let _ = write!(text, "\n{task}");
        }
        for long in self.long_polls() {
            let took = long.took.as_millis();
            let _ = write!(text, "\nдолгий poll: #{} {} {took} мс", long.id, long.name);
        }
        text
    }
}
