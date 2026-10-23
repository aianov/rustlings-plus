//! Пул потоков на std: очередь заданий, результаты через канал, ожидание на Condvar.

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug, PartialEq)]
pub struct PoolClosed;

#[derive(Debug, PartialEq)]
pub enum JobError {
    Panicked,
}

pub struct JobHandle<T> {
    result: Receiver<T>,
}

impl<T> JobHandle<T> {
    pub fn join(self) -> Result<T, JobError> {
        self.result.recv().map_err(|_| JobError::Panicked)
    }
}

struct State {
    queue: VecDeque<Job>,
    running: usize,
    closed: bool,
}

struct Shared {
    state: Mutex<State>,
    work: Condvar,
    idle: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub struct ThreadPool {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

fn worker_loop(shared: &Shared) {
    loop {
        let job = {
            let state = shared.lock();
            let mut state = shared.work.wait_while(state, |state| state.queue.is_empty() && !state.closed).unwrap_or_else(|poisoned| poisoned.into_inner());
            match state.queue.pop_front() {
                Some(job) => {
                    state.running += 1;
                    job
                }
                None => return,
            }
        };
        let _ = panic::catch_unwind(AssertUnwindSafe(job));
        let mut state = shared.lock();
        state.running -= 1;
        if state.running == 0 && state.queue.is_empty() {
            shared.idle.notify_all();
        }
    }
}

impl ThreadPool {
    pub fn new(workers: usize) -> ThreadPool {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { queue: VecDeque::new(), running: 0, closed: false }),
            work: Condvar::new(),
            idle: Condvar::new(),
        });
        let threads = (0..workers.max(1))
            .map(|id| {
                let shared = Arc::clone(&shared);
                thread::Builder::new()
                    .name(format!("worker-{id}"))
                    .spawn(move || worker_loop(&shared))
                    .expect("не удалось запустить рабочий поток")
            })
            .collect();
        ThreadPool { shared, threads }
    }

    pub fn workers(&self) -> usize {
        self.threads.len()
    }

    pub fn execute(&self, job: impl FnOnce() + Send + 'static) -> Result<(), PoolClosed> {
        let mut state = self.shared.lock();
        if state.closed {
            return Err(PoolClosed);
        }
        state.queue.push_back(Box::new(job));
        drop(state);
        self.shared.work.notify_one();
        Ok(())
    }

    pub fn submit<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> Result<JobHandle<T>, PoolClosed> {
        let (sender, result) = mpsc::channel();
        self.execute(move || {
            let _ = sender.send(job());
        })?;
        Ok(JobHandle { result })
    }

    pub fn wait_idle(&self) {
        let state = self.shared.lock();
        let _state = self
            .shared
            .idle
            .wait_while(state, |state| state.running > 0 || !state.queue.is_empty())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }

    pub fn pending(&self) -> usize {
        self.shared.lock().queue.len()
    }

    pub fn close(&self) {
        self.shared.lock().closed = true;
        self.shared.work.notify_all();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.close();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
