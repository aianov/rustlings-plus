
use std::time::Duration;

fn main() {}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn us(n: u64) -> Duration {
        Duration::from_micros(n)
    }

    fn assert_close(real: Duration, estimate: Duration, bits: u32, what: &str) {
        let (r, e) = (real.as_micros() as f64, estimate.as_micros() as f64);
        let bound = r / f64::from(1u32 << bits) + 1.0;
        assert!(e >= r - 1.0 && e - r <= bound, "{what}: настоящее {r} мкс, оценка {e} мкс, допустимая ошибка не больше {bound:.1} мкс");
    }

    #[test]
    fn small_values_are_exact_and_big_ones_close() {
        let mut h = LogHistogram::new(5);
        for v in [3, 17, 31] {
            h.record(us(v));
        }
        assert_eq!(h.value_at_quantile(1.0), us(31), "малые значения лежат в своих корзинах без потери");
        let mut big = LogHistogram::new(5);
        big.record(us(123_456));
        assert_close(us(123_456), big.value_at_quantile(0.5), 5, "одно большое значение");
    }

    #[test]
    fn percentiles_stay_within_the_promised_error() {
        let mut h = LogHistogram::new(7);
        let mut real: Vec<u64> = Vec::new();
        let mut x: u64 = 88172645463325252;
        for _ in 0..50_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let v = 50 + (x % 2_000_000) * (x % 7) / 6;
            real.push(v);
            h.record(us(v));
        }
        real.sort_unstable();
        for q in [0.5, 0.9, 0.99, 0.999] {
            let exact = real[((q * real.len() as f64).ceil() as usize).max(1) - 1];
            assert_close(us(exact), h.value_at_quantile(q), 7, &format!("квантиль {q}"));
        }
        assert_eq!(h.count(), 50_000);
        assert_eq!(h.max(), us(*real.last().expect("не пусто")), "максимум хранится точно");
        assert_eq!(h.min(), us(real[0]), "минимум тоже");
    }

    #[test]
    fn memory_does_not_grow_with_the_number_of_samples() {
        let mut h = LogHistogram::new(6);
        let before = h.buckets_len();
        for i in 0..200_000u64 {
            h.record(us(i * 37 % 5_000_000));
        }
        assert_eq!(h.buckets_len(), before, "число корзин фиксировано и не зависит от замеров");
        assert!(before < 5_000, "корзин {before}: должно быть порядка десятков 2^bits, а не по корзине на микросекунду");
        assert_eq!(h.count(), 200_000);
    }

    #[test]
    fn merging_equals_recording_into_one() {
        let (mut a, mut b, mut whole) = (LogHistogram::new(6), LogHistogram::new(6), LogHistogram::new(6));
        for i in 1..=5_000u64 {
            let v = us(i * i % 90_000 + 10);
            if i % 3 == 0 { a.record(v) } else { b.record(v) }
            whole.record(v);
        }
        a.merge(&b);
        assert_eq!(a.count(), whole.count());
        for q in [0.0, 0.25, 0.5, 0.95, 0.99, 1.0] {
            assert_eq!(a.value_at_quantile(q), whole.value_at_quantile(q), "квантиль {q} после слияния должен совпадать с общей записью");
        }
        assert_eq!((a.min(), a.max()), (whole.min(), whole.max()));
    }

    #[test]
    fn empty_and_zero_and_huge_values_are_survivable() {
        let mut h = LogHistogram::new(4);
        assert_eq!((h.count(), h.value_at_quantile(0.99), h.min(), h.max()), (0, Duration::ZERO, Duration::ZERO, Duration::ZERO));
        h.record(Duration::ZERO);
        h.record(Duration::from_secs(60 * 60 * 24 * 365));
        assert_eq!(h.count(), 2);
        assert_eq!(h.min(), Duration::ZERO);
        assert_eq!(h.value_at_quantile(0.5), Duration::ZERO, "ноль лежит в своей корзине");
        assert!(h.value_at_quantile(1.0) >= Duration::from_secs(60 * 60 * 24 * 365), "год в микросекундах помещается в u64");
    }
}
