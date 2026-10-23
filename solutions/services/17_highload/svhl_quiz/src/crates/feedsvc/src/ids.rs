/// Генератор сортируемых номеров: 42 бита миллисекунд, 10 бит узла, 12 бит счётчика. Часы могут идти назад: номера всё равно растут.
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
        if now > self.last_ms {
            self.last_ms = now;
            self.seq = 0;
        } else {
            self.seq += 1;
            if self.seq == 4096 {
                self.last_ms += 1;
                self.seq = 0;
            }
        }
        (self.last_ms << 22) | (self.node << 12) | self.seq
    }
}
