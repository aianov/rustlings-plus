pub const SUB_BITS: u32 = 7;

const SUB: u64 = 1 << SUB_BITS;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Histogram {
    counts: Vec<u64>,
    total: u64,
    sum: u128,
    max: u64,
    min: Option<u64>,
}

fn index(value: u64) -> usize {
    if value < SUB {
        return usize::try_from(value).unwrap_or(0);
    }
    let shift = 63 - value.leading_zeros() - SUB_BITS;
    let top = value >> shift;
    usize::try_from(u64::from(shift + 1) * SUB + (top - SUB)).unwrap_or(usize::MAX)
}

fn highest(bucket: usize) -> u64 {
    let bucket = bucket as u64;
    if bucket < SUB {
        return bucket;
    }
    let shift = bucket / SUB - 1;
    let top = SUB + bucket % SUB;
    ((top + 1) << shift) - 1
}

impl Histogram {
    pub fn new() -> Histogram {
        Histogram::default()
    }

    pub fn record(&mut self, value: u64) {
        let at = index(value);
        if self.counts.len() <= at {
            self.counts.resize(at + 1, 0);
        }
        self.counts[at] += 1;
        self.total += 1;
        self.sum += u128::from(value);
        self.max = self.max.max(value);
        self.min = Some(self.min.map_or(value, |min| min.min(value)));
    }

    pub fn merge(&mut self, other: &Histogram) {
        if self.counts.len() < other.counts.len() {
            self.counts.resize(other.counts.len(), 0);
        }
        for (own, theirs) in self.counts.iter_mut().zip(&other.counts) {
            *own += theirs;
        }
        self.total += other.total;
        self.sum += other.sum;
        self.max = self.max.max(other.max);
        self.min = match (self.min, other.min) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }

    pub fn count(&self) -> u64 {
        self.total
    }

    pub fn max(&self) -> u64 {
        self.max
    }

    pub fn min(&self) -> u64 {
        self.min.unwrap_or(0)
    }

    pub fn mean(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.sum as f64 / self.total as f64
    }

    pub fn quantile(&self, q: f64) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let rank = ((q.clamp(0.0, 1.0) * self.total as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (bucket, count) in self.counts.iter().enumerate() {
            seen += count;
            if seen >= rank {
                return highest(bucket).min(self.max);
            }
        }
        self.max
    }
}
