use std::any::Any;
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        todo!()
    }

    pub fn next_u64(&mut self) -> u64 {
        todo!()
    }

    pub fn below(&mut self, n: u64) -> u64 {
        todo!()
    }

    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        todo!()
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        todo!()
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        todo!()
    }
}

pub const INTERESTING: [u8; 8] = [0, 1, 0x7f, 0x80, 0xff, b'\r', b'\n', b' '];

pub fn mutate(rng: &mut Rng, input: &[u8], corpus: &[Vec<u8>]) -> Vec<u8> {
    todo!()
}

pub fn shrink(input: &[u8], still_fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    todo!()
}

#[derive(Debug, Clone)]
pub struct Config {
    pub seed: u64,
    pub max_iterations: u64,
    pub max_time: Duration,
}

#[derive(Debug)]
pub struct Crash {
    pub input: Vec<u8>,
    pub original: Vec<u8>,
    pub iteration: u64,
    pub message: String,
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(p) => p.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_else(|| "паника без сообщения".to_string()),
    }
}

pub fn input_for(corpus: &[Vec<u8>], seed: u64, iteration: u64) -> Vec<u8> {
    todo!()
}

pub fn fuzz(cfg: &Config, corpus: &[Vec<u8>], target: impl Fn(&[u8])) -> Option<Crash> {
    todo!()
}

#[derive(Debug)]
pub struct PropFailure {
    pub seed: u64,
    pub message: String,
}

pub fn run_property(cases: u64, base_seed: u64, prop: impl Fn(&mut Rng) -> Result<(), String>) -> Result<(), PropFailure> {
    todo!()
}

#[derive(Debug)]
pub struct SplitFailure<R> {
    pub cuts: Vec<usize>,
    pub whole: R,
    pub split: R,
}

pub fn check_splits<R: PartialEq + Debug>(input: &[u8], run: impl Fn(&[&[u8]]) -> R) -> Result<(), SplitFailure<R>> {
    todo!()
}
