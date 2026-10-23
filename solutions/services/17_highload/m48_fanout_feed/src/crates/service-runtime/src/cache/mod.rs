use std::collections::HashMap;
use std::hash::Hash;

const NIL: usize = usize::MAX;

#[derive(Debug)]
struct Node<K, V> {
    key: K,
    value: V,
    prev: usize,
    next: usize,
}

#[derive(Debug)]
pub struct Lru<K, V> {
    capacity: usize,
    index: HashMap<K, usize>,
    nodes: Vec<Node<K, V>>,
    free: Vec<usize>,
    head: usize,
    tail: usize,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub fn new(capacity: usize) -> Lru<K, V> {
        Lru {
            capacity: capacity.max(1),
            index: HashMap::new(),
            nodes: Vec::new(),
            free: Vec::new(),
            head: NIL,
            tail: NIL,
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn unlink(&mut self, at: usize) {
        let (prev, next) = (self.nodes[at].prev, self.nodes[at].next);
        if prev == NIL {
            self.head = next;
        } else {
            self.nodes[prev].next = next;
        }
        if next == NIL {
            self.tail = prev;
        } else {
            self.nodes[next].prev = prev;
        }
    }

    fn push_front(&mut self, at: usize) {
        self.nodes[at].prev = NIL;
        self.nodes[at].next = self.head;
        if self.head != NIL {
            self.nodes[self.head].prev = at;
        }
        self.head = at;
        if self.tail == NIL {
            self.tail = at;
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let at = *self.index.get(key)?;
        self.unlink(at);
        self.push_front(at);
        Some(&self.nodes[at].value)
    }

    pub fn peek(&self, key: &K) -> Option<&V> {
        self.index.get(key).map(|at| &self.nodes[*at].value)
    }

    pub fn put(&mut self, key: K, value: V) -> Option<K> {
        if let Some(&at) = self.index.get(&key) {
            self.nodes[at].value = value;
            self.unlink(at);
            self.push_front(at);
            return None;
        }
        let mut evicted = None;
        if self.index.len() >= self.capacity {
            let last = self.tail;
            self.unlink(last);
            let old = self.nodes[last].key.clone();
            self.index.remove(&old);
            self.free.push(last);
            evicted = Some(old);
        }
        let node = Node {
            key: key.clone(),
            value,
            prev: NIL,
            next: NIL,
        };
        let at = match self.free.pop() {
            Some(at) => {
                self.nodes[at] = node;
                at
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        self.index.insert(key, at);
        self.push_front(at);
        evicted
    }

    pub fn remove(&mut self, key: &K) -> bool {
        let Some(at) = self.index.remove(key) else {
            return false;
        };
        self.unlink(at);
        self.free.push(at);
        true
    }

    pub fn keys_newest_first(&self) -> Vec<K> {
        let mut keys = Vec::with_capacity(self.len());
        let mut at = self.head;
        while at != NIL {
            keys.push(self.nodes[at].key.clone());
            at = self.nodes[at].next;
        }
        keys
    }
}
