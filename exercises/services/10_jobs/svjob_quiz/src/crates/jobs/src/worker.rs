use crate::policy::{Action, JobError, decide};
use crate::queue::{Delivery, MemQueue};
use std::collections::VecDeque;

pub struct Worker {
    queue: MemQueue,
    prefetch: usize,
    buffer: VecDeque<Delivery>,
    stopped: bool,
}

impl Worker {
    pub fn new(queue: MemQueue, prefetch: usize) -> Worker {
        Worker { queue, prefetch: prefetch.max(1), buffer: VecDeque::new(), stopped: false }
    }

    pub fn queue(&self) -> &MemQueue {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut MemQueue {
        &mut self.queue
    }

    pub fn tick(&mut self, now: u64, handler: &mut impl FnMut(&Delivery) -> Result<(), JobError>) -> bool {
        let _ = (now, handler);
        todo!()
    }

    pub fn shutdown(&mut self, now: u64) {
        let _ = (now,);
        todo!()
    }
}
