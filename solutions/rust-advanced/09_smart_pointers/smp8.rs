use std::cell::{Cell, RefCell};
use std::collections::HashMap;

pub struct Memo {
    cache: RefCell<HashMap<u64, u64>>,
    computed: Cell<u32>,
}

impl Memo {
    pub fn new() -> Memo {
        Memo { cache: RefCell::new(HashMap::new()), computed: Cell::new(0) }
    }

    pub fn computed(&self) -> u32 {
        self.computed.get()
    }

    pub fn fib(&self, n: u64) -> u64 {
        let known = self.cache.borrow().get(&n).copied();
        if let Some(known) = known {
            return known;
        }
        self.computed.set(self.computed.get() + 1);
        let value = if n < 2 { n } else { self.fib(n - 1) + self.fib(n - 2) };
        self.cache.borrow_mut().insert(n, value);
        value
    }
}

impl Default for Memo {
    fn default() -> Memo {
        Memo::new()
    }
}

fn main() {
    println!("{}", Memo::new().fib(10));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values() {
        let m = Memo::new();
        assert_eq!(m.fib(0), 0);
        assert_eq!(m.fib(1), 1);
        assert_eq!(m.fib(10), 55);
    }

    #[test]
    fn deep_recursion_does_not_hit_a_double_borrow() {
        let m = Memo::new();
        assert_eq!(m.fib(50), 12_586_269_025);
        assert_eq!(m.fib(90), 2_880_067_194_370_816_120);
    }

    #[test]
    fn every_number_is_computed_once() {
        let m = Memo::new();
        m.fib(40);
        assert_eq!(m.computed(), 41, "числа от 0 до 40, каждое по одному разу");
        m.fib(40);
        m.fib(10);
        assert_eq!(m.computed(), 41, "повторные запросы берутся из таблицы");
        m.fib(41);
        assert_eq!(m.computed(), 42);
    }
}
