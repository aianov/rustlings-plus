use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use crate::mutate::mutate;
use crate::rng::Rng;
use crate::shrink::shrink;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub seed: u64,
    pub max_iterations: u64,
    pub max_time: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    pub input: Vec<u8>,
    pub original: Vec<u8>,
    pub iteration: u64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropFailure {
    pub seed: u64,
    pub message: String,
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(text) => *text,
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .unwrap_or_else(|| String::from("паника без сообщения")),
    }
}

pub fn input_for(corpus: &[Vec<u8>], seed: u64, iteration: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed.wrapping_add(iteration));
    let base = rng.pick(corpus).cloned().unwrap_or_default();
    mutate(&mut rng, &base, corpus)
}

pub fn fuzz(config: &Config, corpus: &[Vec<u8>], target: impl Fn(&[u8])) -> Option<Crash> {
    let started = Instant::now();
    let panics = |data: &[u8]| catch_unwind(AssertUnwindSafe(|| target(data))).is_err();
    for iteration in 0..config.max_iterations {
        if started.elapsed() >= config.max_time {
            return None;
        }
        let original = input_for(corpus, config.seed, iteration);
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| target(&original))) {
            let input = shrink(&original, panics);
            return Some(Crash {
                input,
                original,
                iteration,
                message: panic_message(payload),
            });
        }
    }
    None
}

pub fn run_property(
    cases: u64,
    base_seed: u64,
    property: impl Fn(&mut Rng) -> Result<(), String>,
) -> Result<(), PropFailure> {
    for case in 0..cases {
        let seed = base_seed.wrapping_add(case);
        if let Err(message) = property(&mut Rng::new(seed)) {
            return Err(PropFailure { seed, message });
        }
    }
    Ok(())
}
