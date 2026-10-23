
use std::time::Duration;

pub struct LogHistogram {
    bits: u32,
    counts: Vec<u64>,
    total: u64,
    min: u64,
    max: u64,
}

fn bucket_index(value: u64, bits: u32) -> usize {
    let linear = 1u64 << bits;
    if value < linear {
        return value as usize;
    }
    let top = 63 - value.leading_zeros();
    let shift = top - bits;
    ((u64::from(shift) + 1) * linear + ((value >> shift) - linear)) as usize
}

fn bucket_highest(index: usize, bits: u32) -> u64 {
    let linear = 1usize << bits;
    if index < linear {
        return index as u64;
    }
    let shift = (index / linear - 1) as u32;
    let lowest = ((linear + index % linear) as u64) << shift;
    lowest + (1u64 << shift) - 1
}

impl LogHistogram {
    pub fn new(bits: u32) -> LogHistogram {
        assert!((1..=12).contains(&bits), "точность от 1 до 12 бит");
        let buckets = (64 - bits as usize + 1) << bits;
        LogHistogram { bits, counts: vec![0; buckets], total: 0, min: u64::MAX, max: 0 }
    }

    pub fn record(&mut self, value: Duration) {
        let micros = u64::try_from(value.as_micros()).unwrap_or(u64::MAX);
        self.counts[bucket_index(micros, self.bits)] += 1;
        self.total += 1;
        self.min = self.min.min(micros);
        self.max = self.max.max(micros);
    }

    pub fn count(&self) -> u64 {
        self.total
    }

    pub fn min(&self) -> Duration {
        if self.total == 0 { Duration::ZERO } else { Duration::from_micros(self.min) }
    }

    pub fn max(&self) -> Duration {
        Duration::from_micros(self.max)
    }

    pub fn buckets_len(&self) -> usize {
        self.counts.len()
    }

    pub fn value_at_quantile(&self, q: f64) -> Duration {
        if self.total == 0 {
            return Duration::ZERO;
        }
        let rank = ((q.clamp(0.0, 1.0) * self.total as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (index, n) in self.counts.iter().enumerate() {
            seen += n;
            if seen >= rank {
                let upper = bucket_highest(index, self.bits).min(self.max).max(self.min);
                return Duration::from_micros(upper);
            }
        }
        Duration::from_micros(self.max)
    }

    pub fn merge(&mut self, other: &LogHistogram) {
        assert_eq!(self.bits, other.bits, "гистограммы разной точности не складываются");
        for (mine, theirs) in self.counts.iter_mut().zip(&other.counts) {
            *mine += theirs;
        }
        self.total += other.total;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}
