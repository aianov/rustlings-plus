// Кэш профилей запомнил всех, кто хоть раз заходил в ленту, и за неделю съел всю память: процесс убит OOM-killer'ом, а кэш
// вместе с ним. Нужен предел размера и правило, кого выбросить. Разумное правило: того, кого дольше всех не трогали (LRU).
// Цель: `Lru` с ёмкостью. Когда места нет, уходит давнее всех использованный; чтение продлевает жизнь записи. Все операции
// должны работать за O(1): кэш на 50 000 записей, в котором на каждую операцию перебирается список, станет узким местом.


use std::collections::HashMap;

fn main() {}

pub struct Lru {
    capacity: usize,
    map: HashMap<u64, String>,
}

impl Lru {
    pub fn new(capacity: usize) -> Lru {
        Lru { capacity, map: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Читает значение и помечает запись как только что использованную.
    pub fn get(&mut self, key: u64) -> Option<&String> {
        self.map.get(&key)
    }

    /// Кладёт значение. Если записей уже `capacity`, уходит давнее всех использованная.
    pub fn put(&mut self, key: u64, value: String) {
        self.map.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn v(n: u64) -> String {
        format!("v{n}")
    }

    #[test]
    fn it_never_grows_beyond_capacity() {
        let mut c = Lru::new(3);
        for i in 0..10 {
            c.put(i, v(i));
        }
        assert_eq!(c.len(), 3, "в кэше {} записей при ёмкости 3", c.len());
    }

    #[test]
    fn the_least_recently_used_goes_first() {
        let mut c = Lru::new(3);
        c.put(1, v(1));
        c.put(2, v(2));
        c.put(3, v(3));
        c.put(4, v(4));
        assert!(c.get(1).is_none(), "запись 1 самая старая и ушла первой");
        assert_eq!(c.get(2).map(String::as_str), Some("v2"));
    }

    #[test]
    fn reading_keeps_an_entry_alive() {
        let mut c = Lru::new(3);
        c.put(1, v(1));
        c.put(2, v(2));
        c.put(3, v(3));
        assert!(c.get(1).is_some());
        c.put(4, v(4));
        assert!(c.get(1).is_some(), "запись 1 только что читали, она свежая");
        assert!(c.get(2).is_none(), "ушла запись 2, давнее всех не тронутая");
    }

    #[test]
    fn overwriting_refreshes_and_does_not_grow() {
        let mut c = Lru::new(2);
        c.put(1, v(1));
        c.put(2, v(2));
        c.put(1, "новое".to_string());
        assert_eq!(c.len(), 2);
        c.put(3, v(3));
        assert_eq!(c.get(1).map(String::as_str), Some("новое"), "перезапись обновляет и свежесть");
        assert!(c.get(2).is_none());
    }

    #[test]
    fn capacity_zero_stores_nothing() {
        let mut c = Lru::new(0);
        c.put(1, v(1));
        assert!(c.is_empty());
        assert!(c.get(1).is_none());
    }

    #[test]
    fn operations_are_constant_time() {
        let started = Instant::now();
        let mut c = Lru::new(50_000);
        for i in 0..200_000u64 {
            c.put(i, v(i));
            if i % 3 == 0 {
                c.get(i / 2 + 40_000);
            }
        }
        assert_eq!(c.len(), 50_000);
        assert!(started.elapsed() < Duration::from_secs(5), "200 000 операций заняли {:?}: на каждой перебирается список", started.elapsed());
    }
}
