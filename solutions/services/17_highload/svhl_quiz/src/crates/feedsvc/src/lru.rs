use std::collections::HashMap;
use std::time::Duration;

const NIL: usize = usize::MAX;

struct Node<V> {
    key: u64,
    value: V,
    prev: usize,
    next: usize,
}

/// Кэш с вытеснением давно не использованного, все операции за O(1).
pub struct Lru<V> {
    capacity: usize,
    map: HashMap<u64, usize>,
    nodes: Vec<Node<V>>,
    free: Vec<usize>,
    head: usize,
    tail: usize,
}

impl<V> Lru<V> {
    pub fn new(capacity: usize) -> Lru<V> {
        Lru { capacity, map: HashMap::new(), nodes: Vec::new(), free: Vec::new(), head: NIL, tail: NIL }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get(&mut self, key: u64) -> Option<&V> {
        let idx = *self.map.get(&key)?;
        self.unlink(idx);
        self.push_front(idx);
        Some(&self.nodes[idx].value)
    }

    pub fn put(&mut self, key: u64, value: V) {
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

    pub fn remove(&mut self, key: u64) {
        if let Some(idx) = self.map.remove(&key) {
            self.unlink(idx);
            self.free.push(idx);
        }
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

/// Срок жизни записи для ключа: базовый плюс-минус 10%, один и тот же для одного ключа.
pub fn ttl_for(base: Duration, key: u64) -> Duration {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.to_le_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    let spread = (h >> 11) as f64 / (1u64 << 53) as f64;
    base.mul_f64(0.9 + 0.2 * spread)
}
