
use std::collections::HashMap;

fn main() {}

pub struct Lru {
    capacity: usize,
    map: HashMap<u64, usize>,
    nodes: Vec<Node>,
    free: Vec<usize>,
    head: usize,
    tail: usize,
}

const NIL: usize = usize::MAX;

struct Node {
    key: u64,
    value: String,
    prev: usize,
    next: usize,
}

impl Lru {
    pub fn new(capacity: usize) -> Lru {
        Lru { capacity, map: HashMap::new(), nodes: Vec::new(), free: Vec::new(), head: NIL, tail: NIL }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Читает значение и помечает запись как только что использованную.
    pub fn get(&mut self, key: u64) -> Option<&String> {
        let idx = *self.map.get(&key)?;
        self.unlink(idx);
        self.push_front(idx);
        Some(&self.nodes[idx].value)
    }

    /// Кладёт значение. Если записей уже `capacity`, уходит давнее всех использованная.
    pub fn put(&mut self, key: u64, value: String) {
        if let Some(&idx) = self.map.get(&key) {
            self.nodes[idx].value = value;
            self.unlink(idx);
            self.push_front(idx);
            return;
        }
        if self.capacity == 0 {
            return;
        }
        if self.map.len() == self.capacity {
            let last = self.tail;
            self.unlink(last);
            let old = self.nodes[last].key;
            self.map.remove(&old);
            self.free.push(last);
        }
        let node = Node { key, value, prev: NIL, next: NIL };
        let idx = match self.free.pop() {
            Some(slot) => {
                self.nodes[slot] = node;
                slot
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        self.map.insert(key, idx);
        self.push_front(idx);
    }

    fn unlink(&mut self, idx: usize) {
        let (prev, next) = (self.nodes[idx].prev, self.nodes[idx].next);
        if prev == NIL { self.head = next } else { self.nodes[prev].next = next }
        if next == NIL { self.tail = prev } else { self.nodes[next].prev = prev }
    }

    fn push_front(&mut self, idx: usize) {
        self.nodes[idx].prev = NIL;
        self.nodes[idx].next = self.head;
        if self.head != NIL {
            self.nodes[self.head].prev = idx;
        }
        self.head = idx;
        if self.tail == NIL {
            self.tail = idx;
        }
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
