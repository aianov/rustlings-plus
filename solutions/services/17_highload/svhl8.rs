
fn main() {}

pub struct IdGen<C: Fn() -> u64> {
    node: u64,
    clock: C,
    last_ms: u64,
    seq: u64,
}

impl<C: Fn() -> u64> IdGen<C> {
    /// `clock` возвращает текущее время в миллисекундах. Узел от 0 до 1023.
    pub fn new(node: u64, clock: C) -> IdGen<C> {
        assert!(node < 1024, "номер узла не помещается в 10 бит");
        IdGen { node, clock, last_ms: 0, seq: 0 }
    }

    pub fn generate(&mut self) -> u64 {
        let mut now = (self.clock)();
        if now == self.last_ms {
            self.seq += 1;
            if self.seq == 4096 {
                while now <= self.last_ms {
                    now = (self.clock)();
                }
                self.last_ms = now;
                self.seq = 0;
            }
        } else {
            self.last_ms = now;
            self.seq = 0;
        }
        (self.last_ms << 22) | (self.node << 12) | self.seq
    }
}

/// Раскладывает номер: (миллисекунды, узел, счётчик).
pub fn parts(id: u64) -> (u64, u64, u64) {
    (id >> 22, (id >> 12) & 0x3ff, id & 0xfff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashSet;

    #[test]
    fn ids_grow_within_one_millisecond() {
        let mut g = IdGen::new(5, || 1_000);
        let ids: Vec<u64> = (0..100).map(|_| g.generate()).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "номера в одну миллисекунду должны строго расти: {:?}", &ids[..4]);
        assert_eq!(parts(ids[0]), (1_000, 5, 0));
        assert_eq!(parts(ids[99]), (1_000, 5, 99));
    }

    #[test]
    fn the_counter_overflow_waits_for_the_next_millisecond() {
        let calls = Cell::new(0u64);
        let mut g = IdGen::new(1, || {
            calls.set(calls.get() + 1);
            7 + calls.get() / 5000
        });
        let ids: Vec<u64> = (0..10_000).map(|_| g.generate()).collect();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 10_000, "при исчерпании счётчика номера повторились (счётчик 4096 значений, а часы перешли на новую миллисекунду только после ожидания)");
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "и порядок обязан сохраняться");
    }

    #[test]
    fn a_new_millisecond_resets_the_counter() {
        let now = Cell::new(10u64);
        let mut g = IdGen::new(2, || now.get());
        let a = g.generate();
        let b = g.generate();
        now.set(11);
        let c = g.generate();
        assert_eq!((parts(a).2, parts(b).2, parts(c).2), (0, 1, 0));
        assert!(a < b && b < c);
    }

    #[test]
    fn ids_sort_by_time_then_never_collide_across_nodes() {
        let now = Cell::new(100u64);
        let mut a = IdGen::new(1, || now.get());
        let mut b = IdGen::new(2, || now.get());
        let early = a.generate();
        now.set(101);
        let late = b.generate();
        assert!(early < late, "номер, выданный раньше по времени, обязан быть меньше, какой бы узел его ни выдал");
        let (x, y) = (a.generate(), b.generate());
        assert_ne!(x, y, "два узла в одну миллисекунду не должны выдать одинаковый номер");
        assert_eq!((parts(x).1, parts(y).1), (1, 2));
    }
}
