#![allow(unused_variables, dead_code)]

mod conn;
mod envelope;
mod hub;
mod methods;
mod router;
mod server;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub use server::run;

#[derive(Debug, Clone)]
pub struct Config {
    pub outbox: usize,
    pub ping_every: Duration,
    pub idle_limit: Duration,
    pub max_in_flight: usize,
    pub grace: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config { outbox: 64, ping_every: Duration::from_secs(30), idle_limit: Duration::from_secs(90), max_in_flight: 8, grace: Duration::from_secs(5) }
    }
}

pub trait Tokens: Send + Sync + 'static {
    fn check(&self, token: &str) -> Option<String>;
}

#[derive(Default)]
pub struct Stats {
    connections: AtomicUsize,
    kicked: AtomicUsize,
    timed_out: AtomicUsize,
}

impl Stats {
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    pub fn kicked(&self) -> usize {
        self.kicked.load(Ordering::SeqCst)
    }

    pub fn timed_out(&self) -> usize {
        self.timed_out.load(Ordering::SeqCst)
    }

    pub(crate) fn connection_opened(&self) {
        self.connections.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn connection_closed(&self) {
        self.connections.fetch_sub(1, Ordering::SeqCst);
    }

    pub(crate) fn note_kicked(&self) {
        self.kicked.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn note_timed_out(&self) {
        self.timed_out.fetch_add(1, Ordering::SeqCst);
    }
}
