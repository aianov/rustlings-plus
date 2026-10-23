use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub struct Overloaded {
    pub retry_after: Duration,
}

pub struct Shedder {
    max_in_flight: usize,
    retry_after: Duration,
    in_flight: Arc<AtomicUsize>,
}

pub struct Admit {
    counter: Arc<AtomicUsize>,
}

impl Drop for Admit {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Shedder {
    pub fn new(max_in_flight: usize, retry_after: Duration) -> Shedder {
        Shedder { max_in_flight, retry_after, in_flight: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    pub fn try_admit(&self) -> Result<Admit, Overloaded> {
        self.in_flight
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < self.max_in_flight).then_some(n + 1))
            .map(|_| Admit { counter: self.in_flight.clone() })
            .map_err(|_| Overloaded { retry_after: self.retry_after })
    }
}
