// Генератор хранит каждое время ответа в списке: за час нагрузки с десятками тысяч запросов в секунду список съедает гигабайты,
// а слияние статистик воркеров копирует всё. Цель: гистограмма, которая занимает фиксированную память при любом числе замеров и
// даёт перцентиль с заданной относительной точностью. Значения в микросекундах делятся на корзины так: малые числа лежат
// по одному на корзину, а каждый следующий диапазон от степени двойки до следующей делится на `2^bits` равных корзин. Ошибка
// перцентиля не больше `1/2^bits` от значения. Нужно записывать, считать, находить значение по квантилю и сливать две гистограммы.


use std::time::Duration;

fn main() {}

pub struct LogHistogram {
    // TODO: что здесь хранится, решаешь ты.
}

impl LogHistogram {
    pub fn new(bits: u32) -> LogHistogram {
        let _ = bits;
        todo!()
    }

    pub fn record(&mut self, value: Duration) {
        let _ = value;
        todo!()
    }

    pub fn count(&self) -> u64 {
        todo!()
    }

    pub fn min(&self) -> Duration {
        todo!()
    }

    pub fn max(&self) -> Duration {
        todo!()
    }

    pub fn buckets_len(&self) -> usize {
        todo!()
    }

    pub fn value_at_quantile(&self, q: f64) -> Duration {
        let _ = q;
        todo!()
    }

    pub fn merge(&mut self, other: &LogHistogram) {
        let _ = other;
        todo!()
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
