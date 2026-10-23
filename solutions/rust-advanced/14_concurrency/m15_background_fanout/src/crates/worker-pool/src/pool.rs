use std::collections::VecDeque;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use crate::error::PoolClosed;
use crate::handle::JobHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

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
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn next_job(&self) -> Option<Job> {
        let state = self.lock();
        let mut state = self
            .work
            .wait_while(state, |state| state.queue.is_empty() && !state.closed)
            .unwrap_or_else(PoisonError::into_inner);
        let job = state.queue.pop_front()?;
        state.running += 1;
        Some(job)
    }

    fn finish_job(&self) {
        let mut state = self.lock();
        state.running -= 1;
        if state.running == 0 && state.queue.is_empty() {
            self.idle.notify_all();
        }
    }
}

fn work(shared: &Shared) {
    while let Some(job) = shared.next_job() {
        let _ = panic::catch_unwind(AssertUnwindSafe(job));
        shared.finish_job();
    }
}

pub struct ThreadPool {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    pub fn new(workers: usize) -> io::Result<ThreadPool> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: VecDeque::new(),
                running: 0,
                closed: false,
            }),
            work: Condvar::new(),
            idle: Condvar::new(),
        });
        let mut pool = ThreadPool {
            shared,
            threads: Vec::new(),
        };
        for id in 0..workers.max(1) {
            let shared = Arc::clone(&pool.shared);
            let thread = thread::Builder::new()
                .name(format!("worker-{id}"))
                .spawn(move || work(&shared))?;
            pool.threads.push(thread);
        }
        Ok(pool)
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

    pub fn submit<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Result<JobHandle<T>, PoolClosed> {
        let (sender, result) = mpsc::channel();
        self.execute(move || {
            let _ = sender.send(job());
        })?;
        Ok(JobHandle::new(result))
    }

    pub fn wait_idle(&self) {
        let state = self.shared.lock();
        let _idle = self
            .shared
            .idle
            .wait_while(state, |state| state.running > 0 || !state.queue.is_empty())
            .unwrap_or_else(PoisonError::into_inner);
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
