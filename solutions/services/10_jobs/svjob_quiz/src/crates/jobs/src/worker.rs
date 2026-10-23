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
        if self.stopped {
            return false;
        }
        while self.buffer.len() < self.prefetch {
            match self.queue.take(now) {
                Some(delivery) => self.buffer.push_back(delivery),
                None => break,
            }
        }
        let Some(delivery) = self.buffer.pop_front() else { return false };
        if delivery.job.run_at > now {
            self.queue.nak(&delivery.job.id, delivery.job.run_at - now, now);
            return true;
        }
        match decide(handler(&delivery), delivery.delivered) {
            Action::Done => self.queue.ack(&delivery.job.id),
            Action::Retry { after_ms } => self.queue.nak(&delivery.job.id, after_ms, now),
            Action::Dead { reason } => self.queue.term(&delivery.job.id, &reason),
        }
        true
    }

    pub fn shutdown(&mut self, now: u64) {
        self.stopped = true;
        while let Some(delivery) = self.buffer.pop_front() {
            self.queue.nak(&delivery.job.id, 0, now);
        }
    }
}
