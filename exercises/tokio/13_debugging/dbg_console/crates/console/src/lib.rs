use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;

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

#[derive(Clone, Default)]
pub struct Console {}

impl Console {
    pub fn new() -> Console {
        todo!()
    }

    pub fn spawn<F>(&self, name: &str, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        todo!()
    }

    pub fn on_long_poll<H>(&self, threshold: Duration, hook: H)
    where
        H: Fn(&str, Duration) + Send + Sync + 'static,
    {
        todo!()
    }

    pub fn tasks(&self) -> Vec<TaskInfo> {
        todo!()
    }

    pub fn alive(&self) -> usize {
        todo!()
    }

    pub fn stuck(&self, longer_than: Duration) -> Vec<String> {
        todo!()
    }

    pub fn prune(&self) -> usize {
        todo!()
    }

    pub fn dump(&self) -> String {
        todo!()
    }
}
