use std::fmt;
use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy {
    pub limit: usize,
}

impl fmt::Display for Busy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "к зависимости уже {} вызовов, новый отклонён сразу",
            self.limit
        )
    }
}

impl std::error::Error for Busy {}

#[derive(Debug, Clone)]
pub struct Bulkhead {
    limit: usize,
    seats: Arc<Semaphore>,
}

#[derive(Debug)]
pub struct Seat {
    _permit: OwnedSemaphorePermit,
}

impl Bulkhead {
    pub fn new(limit: usize) -> Bulkhead {
        Bulkhead {
            limit: limit.max(1),
            seats: Arc::new(Semaphore::new(limit.max(1))),
        }
    }

    pub fn try_enter(&self) -> Result<Seat, Busy> {
        Arc::clone(&self.seats)
            .try_acquire_owned()
            .map(|permit| Seat { _permit: permit })
            .map_err(|_| Busy { limit: self.limit })
    }

    pub fn in_use(&self) -> usize {
        self.limit - self.seats.available_permits()
    }
}
