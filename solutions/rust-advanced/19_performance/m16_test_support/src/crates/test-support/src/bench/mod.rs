use std::fmt;
use std::hint::black_box;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    sorted: Vec<Duration>,
}

impl Summary {
    pub fn from_samples(mut samples: Vec<Duration>) -> Option<Summary> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_unstable();
        Some(Summary { sorted: samples })
    }

    pub fn runs(&self) -> usize {
        self.sorted.len()
    }

    pub fn percentile(&self, p: u32) -> Duration {
        let p = p.clamp(1, 100) as usize;
        let rank = (p * self.sorted.len()).div_ceil(100);
        self.sorted
            .get(rank.saturating_sub(1))
            .copied()
            .unwrap_or_default()
    }

    pub fn median(&self) -> Duration {
        self.percentile(50)
    }

    pub fn p99(&self) -> Duration {
        self.percentile(99)
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "медиана {:.1} мкс, p99 {:.1} мкс, замеров {}",
            micros(self.median()),
            micros(self.p99()),
            self.runs()
        )
    }
}

fn micros(time: Duration) -> f64 {
    time.as_secs_f64() * 1_000_000.0
}

pub fn measure<R>(warmup: usize, runs: usize, mut f: impl FnMut() -> R) -> Option<Summary> {
    for _ in 0..warmup {
        black_box(f());
    }
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        black_box(f());
        samples.push(start.elapsed());
    }
    Summary::from_samples(samples)
}
