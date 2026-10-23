use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::time::Instant;

use super::{Ended, Registry, Slot};

pub struct Traced<F> {
    future: Pin<Box<F>>,
    slot: Arc<Slot>,
    registry: Arc<Registry>,
    done: bool,
}

impl<F> Traced<F> {
    pub(super) fn new(future: F, slot: Arc<Slot>, registry: Arc<Registry>) -> Traced<F> {
        Traced {
            future: Box::pin(future),
            slot,
            registry,
            done: false,
        }
    }

    pub fn id(&self) -> u64 {
        self.slot.id
    }

    pub fn name(&self) -> &'static str {
        self.slot.name
    }
}

struct Polling<'a> {
    slot: &'a Slot,
    armed: bool,
}

impl Drop for Polling<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.slot.progress().panicked = true;
        }
    }
}

impl<F: Future> Future for Traced<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        {
            let mut progress = this.slot.progress();
            progress.polls += 1;
            progress.polling = true;
            progress.since = Instant::now();
        }
        let started = std::time::Instant::now();
        let mut polling = Polling {
            slot: &this.slot,
            armed: true,
        };
        let out = this.future.as_mut().poll(cx);
        polling.armed = false;
        let took = started.elapsed();
        {
            let mut progress = this.slot.progress();
            progress.polling = false;
            progress.since = Instant::now();
            progress.longest = progress.longest.max(took);
        }
        this.registry.slow(&this.slot, took);
        if out.is_ready() {
            this.done = true;
        }
        out
    }
}

impl<F> Drop for Traced<F> {
    fn drop(&mut self) {
        let ended = if self.done {
            Ended::Finished
        } else if self.slot.progress().panicked {
            Ended::Panicked
        } else {
            Ended::Cancelled
        };
        self.registry.end(self.slot.id, ended);
    }
}
