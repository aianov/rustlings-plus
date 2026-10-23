// Генератор номеров постов работал год. Потом на одном узле синхронизация времени (NTP) отмотала часы на 3 миллисекунды назад,
// и в базе появились два поста с одинаковым номером: счётчик в «старой» миллисекунде начал считать заново с нуля, а такие же значения
// там уже выдавались. Часы так делают постоянно: отмотка, високосная секунда, ручная правка. Ронять сервис нельзя, повторять
// номера нельзя, откатывать порядок тоже.
// Цель: `IdGen::generate` выдаёт строго возрастающие номера при любом поведении часов. Пока часы отстают от последнего выданного
// времени, номера продолжают расти от него; когда часы догнали, всё идёт по ним.


fn main() {}

pub struct IdGen<C: Fn() -> u64> {
    node: u64,
    clock: C,
    last_ms: u64,
    seq: u64,
}

impl<C: Fn() -> u64> IdGen<C> {
    pub fn new(node: u64, clock: C) -> IdGen<C> {
        assert!(node < 1024, "номер узла не помещается в 10 бит");
        IdGen { node, clock, last_ms: 0, seq: 0 }
    }

    pub fn generate(&mut self) -> u64 {
        let now = (self.clock)();
        if now == self.last_ms {
            self.seq += 1;
            if self.seq == 4096 {
                self.last_ms += 1;
                self.seq = 0;
            }
        } else {
            self.last_ms = now;
            self.seq = 0;
        }
        (self.last_ms << 22) | (self.node << 12) | self.seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::HashSet;

    #[test]
    fn a_small_rollback_does_not_repeat_ids() {
        let now = Cell::new(1_000u64);
        let mut g = IdGen::new(3, || now.get());
        let mut ids = vec![g.generate(), g.generate(), g.generate()];
        now.set(997);
        ids.extend((0..5).map(|_| g.generate()));
        now.set(1_001);
        ids.extend((0..3).map(|_| g.generate()));
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "после отката часов появились одинаковые номера: {ids:?}");
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "порядок нарушен: {ids:?}");
    }

    #[test]
    fn a_huge_rollback_is_survived_too() {
        let now = Cell::new(5_000_000u64);
        let mut g = IdGen::new(3, || now.get());
        let mut ids: Vec<u64> = (0..10).map(|_| g.generate()).collect();
        now.set(1_400_000);
        ids.extend((0..6000).map(|_| g.generate()));
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "часы отмотали на час, а номера обязаны расти");
    }

    #[test]
    fn when_the_clock_catches_up_ids_follow_it() {
        let now = Cell::new(100u64);
        let mut g = IdGen::new(3, || now.get());
        g.generate();
        now.set(90);
        let during = g.generate();
        now.set(200);
        let after = g.generate();
        assert!(during < after);
        assert_eq!(after >> 22, 200, "часы догнали, номер снова считается по ним");
        assert_eq!(after & 0xfff, 0, "новая миллисекунда начинает счётчик с нуля");
    }

    #[test]
    fn normal_operation_is_unchanged() {
        let now = Cell::new(10u64);
        let mut g = IdGen::new(1, || now.get());
        let a = g.generate();
        let b = g.generate();
        assert_eq!((a >> 22, a & 0xfff, b & 0xfff), (10, 0, 1));
    }
}
