use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

#[derive(Debug, PartialEq)]
pub struct Closed;

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

pub struct Pool {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

fn worker_loop(shared: &Shared) {
    loop {
        let job = {
            let state = shared.state.lock().unwrap();
            let mut state = shared.work.wait_while(state, |state| state.queue.is_empty() && !state.closed).unwrap();
            match state.queue.pop_front() {
                Some(job) => {
                    state.running += 1;
                    job
                }
                None => return,
            }
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
        let mut state = shared.state.lock().unwrap();
        state.running -= 1;
        if state.running == 0 && state.queue.is_empty() {
            shared.idle.notify_all();
        }
    }
}

impl Pool {
    pub fn new(workers: usize) -> Pool {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { queue: VecDeque::new(), running: 0, closed: false }),
            work: Condvar::new(),
            idle: Condvar::new(),
        });
        let threads = (0..workers.max(1))
            .map(|_| {
                let shared = Arc::clone(&shared);
                thread::spawn(move || worker_loop(&shared))
            })
            .collect();
        Pool { shared, threads }
    }

    pub fn execute(&self, job: impl FnOnce() + Send + 'static) -> Result<(), Closed> {
        let mut state = self.shared.state.lock().unwrap();
        if state.closed {
            return Err(Closed);
        }
        state.queue.push_back(Box::new(job));
        drop(state);
        self.shared.work.notify_one();
        Ok(())
    }

    pub fn close(&self) {
        self.shared.state.lock().unwrap().closed = true;
        self.shared.work.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        self.shared.state.lock().unwrap().closed
    }

    pub fn wait_idle(&self) {
        let state = self.shared.state.lock().unwrap();
        let _state = self.shared.idle.wait_while(state, |state| state.running > 0 || !state.queue.is_empty()).unwrap();
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.close();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
