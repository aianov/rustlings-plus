// Мемоизатор чисел Фибоначчи прячет таблицу в `RefCell`, чтобы `fib` работала по общей ссылке. Любая
// проверка на маленьких числах проходит, а `fib(20)` падает с паникой «already borrowed: BorrowMutError».
// Заём таблицы взят в начале функции и живёт до самого конца, а рекурсивный вызов внутри снова просит
// `borrow_mut`. Компилятор такое не видит: за заёмами `RefCell` следят только во время работы.
// TODO: Почини `fib`, чтобы таблица занималась только на короткие обращения, а каждое число считалось ровно один раз.
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
        let mut cache = self.cache.borrow_mut();
        if let Some(&known) = cache.get(&n) {
            return known;
        }
        self.computed.set(self.computed.get() + 1);
        let value = if n < 2 { n } else { self.fib(n - 1) + self.fib(n - 2) };
        cache.insert(n, value);
        value
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
