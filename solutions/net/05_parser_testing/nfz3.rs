#[derive(Debug, Clone)]
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0) не имеет смысла");
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < limit {
                return x % n;
            }
        }
    }

    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

fn main() {
    let mut rng = Rng::new(42);
    println!("{} {}", rng.next_u64(), rng.below(10));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_splitmix64_sequence() {
        let mut rng = Rng::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF, "первое число splitmix64 с seed 0");
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn same_seed_same_stream_different_seed_different() {
        let a: Vec<u64> = { let mut r = Rng::new(7); (0..20).map(|_| r.next_u64()).collect() };
        let b: Vec<u64> = { let mut r = Rng::new(7); (0..20).map(|_| r.next_u64()).collect() };
        let c: Vec<u64> = { let mut r = Rng::new(8); (0..20).map(|_| r.next_u64()).collect() };
        assert_eq!(a, b, "один seed обязан давать одну последовательность");
        assert_ne!(a, c);
    }

    #[test]
    fn below_stays_in_bounds_and_covers() {
        let mut r = Rng::new(1);
        let mut seen = [false; 5];
        for _ in 0..1000 {
            let x = r.below(5);
            assert!(x < 5, "below(5) вернул {x}");
            seen[x as usize] = true;
        }
        assert!(seen.iter().all(|s| *s), "за 1000 попыток должны были выпасть все значения 0..5: {seen:?}");
        assert_eq!(Rng::new(3).below(1), 0);
    }

    #[test]
    #[should_panic]
    fn below_zero_panics() {
        Rng::new(1).below(0);
    }

    #[test]
    fn below_is_not_biased() {
        let n = (1u64 << 63) + (1 << 62);
        let mut r = Rng::new(99);
        let low = (0..20_000).filter(|_| r.below(n) < (1 << 62)).count();
        let ideal = 20_000.0 * (1u64 << 62) as f64 / n as f64;
        assert!(((low as f64) - ideal).abs() < 0.04 * 20_000.0, "доля попаданий {low} далека от ожидаемых {ideal:.0}: остаток от деления перекашивает распределение, нужен отказ от «хвоста»");
    }

    #[test]
    fn range_chance_pick() {
        let mut r = Rng::new(5);
        for _ in 0..500 {
            let x = r.range(10, 13);
            assert!((10..13).contains(&x), "range(10, 13) вернул {x}");
        }
        let hits = (0..10_000).filter(|_| r.chance(30)).count();
        assert!((2500..3500).contains(&hits), "chance(30) сработал {hits} раз из 10000");
        assert!((0..100).all(|_| !Rng::new(1).chance(0)), "chance(0) никогда не срабатывает");
        assert!((0..100).all(|i| Rng::new(i).chance(100)), "chance(100) срабатывает всегда");
        let items = ["a", "b", "c"];
        let picked: std::collections::HashSet<_> = (0..200).map(|_| *r.pick(&items)).collect();
        assert_eq!(picked.len(), 3);
    }
}
