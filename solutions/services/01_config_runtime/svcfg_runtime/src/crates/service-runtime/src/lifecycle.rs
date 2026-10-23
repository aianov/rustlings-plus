use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep};

pub struct State {
    pub ready: AtomicBool,
    pub accepting: AtomicBool,
    pub in_flight: AtomicUsize,
}

pub struct RequestGuard(Arc<State>);

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

pub fn begin_request(state: &Arc<State>) -> Option<RequestGuard> {
    if !state.accepting.load(Ordering::SeqCst) {
        return None;
    }
    state.in_flight.fetch_add(1, Ordering::SeqCst);
    Some(RequestGuard(Arc::clone(state)))
}

#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub drained: bool,
}

pub struct Lifecycle {
    state: Arc<State>,
    background: Vec<JoinHandle<()>>,
}

impl Lifecycle {
    pub fn new(background: Vec<JoinHandle<()>>) -> Lifecycle {
        let state = State {
            ready: AtomicBool::new(true),
            accepting: AtomicBool::new(true),
            in_flight: AtomicUsize::new(0),
        };
        Lifecycle { state: Arc::new(state), background }
    }

    pub fn state(&self) -> Arc<State> {
        Arc::clone(&self.state)
    }

    pub async fn shutdown(self, grace: Duration, drain_deadline: Duration) -> Outcome {
        self.state.ready.store(false, Ordering::SeqCst);
        sleep(grace).await;
        self.state.accepting.store(false, Ordering::SeqCst);
        let deadline = Instant::now() + drain_deadline;
        let drained = loop {
            if self.state.in_flight.load(Ordering::SeqCst) == 0 {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            sleep(Duration::from_millis(5)).await;
        };
        for handle in &self.background {
            handle.abort();
        }
        for handle in self.background {
            let _ = handle.await;
        }
        Outcome { drained }
    }
}
