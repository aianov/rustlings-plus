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
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0) не имеет смысла");
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < limit {
                return x % n;
            }
        }
    }

    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo)
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

pub const INTERESTING: [u8; 8] = [0, 1, 0x7f, 0x80, 0xff, b'\r', b'\n', b' '];


pub fn mutate(rng: &mut Rng, input: &[u8], corpus: &[Vec<u8>]) -> Vec<u8> {
    let mut out = input.to_vec();
    for _ in 0..1 + rng.below(3) {
        let len = out.len() as u64;
        match rng.below(7) {
            0 if len > 0 => {
                let i = rng.below(len) as usize;
                out[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len) as usize;
                out[i] = rng.next_u64() as u8;
            }
            2 => {
                let i = rng.below(len + 1) as usize;
                for _ in 0..1 + rng.below(4) {
                    out.insert(i, rng.next_u64() as u8);
                }
            }
            3 if len > 0 => {
                let i = rng.below(len) as usize;
                let n = 1 + rng.below(8.min(len - i as u64)) as usize;
                out.drain(i..i + n);
            }
            4 if len > 0 => {
                let i = rng.below(len) as usize;
                let n = 1 + rng.below(8.min(len - i as u64)) as usize;
                let piece = out[i..i + n].to_vec();
                out.splice(i..i, piece);
            }
            5 if !corpus.is_empty() => {
                let other = rng.pick(corpus);
                if !other.is_empty() {
                    let a = rng.below(other.len() as u64) as usize;
                    let n = 1 + rng.below((other.len() - a) as u64) as usize;
                    let at = rng.below(len + 1) as usize;
                    out.splice(at..at, other[a..a + n].iter().copied());
                }
            }
            _ => {
                let value = *rng.pick(&INTERESTING);
                if len == 0 {
                    out.push(value);
                } else {
                    let i = rng.below(len) as usize;
                    out[i] = value;
                }
            }
        }
    }
    out
}


pub fn shrink(input: &[u8], still_fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut cur = input.to_vec();
    if !still_fails(&cur) {
        return cur;
    }
    let mut chunk = (cur.len() / 2).max(1);
    loop {
        let mut progress = false;
        let mut start = 0;
        while start < cur.len() {
            let end = (start + chunk).min(cur.len());
            let mut candidate = cur[..start].to_vec();
            candidate.extend_from_slice(&cur[end..]);
            if still_fails(&candidate) {
                cur = candidate;
                progress = true;
            } else {
                start += chunk;
            }
        }
        if chunk > 1 {
            chunk = (chunk / 2).max(1);
        } else if !progress {
            break;
        }
    }
    for i in 0..cur.len() {
        if cur[i] != b'a' {
            let mut candidate = cur.clone();
            candidate[i] = b'a';
            if still_fails(&candidate) {
                cur = candidate;
            }
        }
    }
    cur
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
    let mut rng = Rng::new(seed.wrapping_add(iteration));
    let base = if corpus.is_empty() { Vec::new() } else { rng.pick(corpus).clone() };
    mutate(&mut rng, &base, corpus)
}

pub fn fuzz(cfg: &Config, corpus: &[Vec<u8>], target: impl Fn(&[u8])) -> Option<Crash> {
    let started = Instant::now();
    let panics = |data: &[u8]| catch_unwind(AssertUnwindSafe(|| target(data))).is_err();
    for iteration in 0..cfg.max_iterations {
        if started.elapsed() >= cfg.max_time {
            return None;
        }
        let original = input_for(corpus, cfg.seed, iteration);
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| target(&original))) {
            let input = shrink(&original, panics);
            return Some(Crash { input, original, iteration, message: panic_message(payload) });
        }
    }
    None
}

#[derive(Debug)]
pub struct PropFailure {
    pub seed: u64,
    pub message: String,
}

pub fn run_property(cases: u64, base_seed: u64, prop: impl Fn(&mut Rng) -> Result<(), String>) -> Result<(), PropFailure> {
    for i in 0..cases {
        let seed = base_seed.wrapping_add(i);
        if let Err(message) = prop(&mut Rng::new(seed)) {
            return Err(PropFailure { seed, message });
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct SplitFailure<R> {
    pub cuts: Vec<usize>,
    pub whole: R,
    pub split: R,
}

pub fn check_splits<R: PartialEq + Debug>(input: &[u8], run: impl Fn(&[&[u8]]) -> R) -> Result<(), SplitFailure<R>> {
    let whole = run(&[input]);
    let mut attempts: Vec<Vec<usize>> = (1..input.len()).map(|cut| vec![cut]).collect();
    attempts.push((1..input.len()).collect());
    for cuts in attempts {
        let mut chunks = Vec::new();
        let mut prev = 0;
        for &cut in &cuts {
            chunks.push(&input[prev..cut]);
            prev = cut;
        }
        chunks.push(&input[prev..]);
        let split = run(&chunks);
        if split != whole {
            return Err(SplitFailure { cuts, whole, split });
        }
    }
    Ok(())
}
