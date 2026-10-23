use std::collections::VecDeque;

#[derive(Default)]
pub struct DeliveryQueue {
    items: VecDeque<String>,
}

pub struct Recent<T> {
    cap: usize,
    items: VecDeque<T>,
}

impl DeliveryQueue {
    pub fn push(&mut self, item: &str) {
        self.items.push_back(item.to_string());
    }

    pub fn take(&mut self) -> Option<String> {
        self.items.pop_front()
    }

    pub fn retry_later(&mut self, item: String) {
        self.items.push_front(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Clone> Recent<T> {
    pub fn new(cap: usize) -> Recent<T> {
        Recent { cap, items: VecDeque::with_capacity(cap) }
    }

    pub fn push(&mut self, item: T) {
        if self.cap == 0 {
            return;
        }
        if self.items.len() == self.cap {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    pub fn snapshot(&self) -> Vec<T> {
        self.items.iter().cloned().collect()
    }

    pub fn last(&self, n: usize) -> Vec<T> {
        let skip = self.items.len().saturating_sub(n);
        self.items.iter().skip(skip).cloned().collect()
    }
}

fn main() {
    let mut q = DeliveryQueue::default();
    q.push("hello");
    println!("{:?}", q.take());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_is_first_in_first_out() {
        let mut q = DeliveryQueue::default();
        q.push("a");
        q.push("b");
        q.push("c");
        assert_eq!(q.len(), 3);
        assert_eq!(q.take().as_deref(), Some("a"));
        assert_eq!(q.take().as_deref(), Some("b"));
        assert_eq!(q.take().as_deref(), Some("c"));
        assert_eq!(q.take(), None);
        assert!(q.is_empty());
    }

    #[test]
    fn a_failed_delivery_goes_to_the_front() {
        let mut q = DeliveryQueue::default();
        q.push("a");
        q.push("b");
        let failed = q.take().unwrap();
        q.retry_later(failed);
        assert_eq!(q.take().as_deref(), Some("a"), "неудавшееся сообщение должно уйти первым, а не встать в хвост");
        assert_eq!(q.take().as_deref(), Some("b"));
    }

    #[test]
    fn recent_keeps_only_the_newest() {
        let mut r = Recent::new(3);
        for i in 1..=5 {
            r.push(i);
        }
        assert_eq!(r.snapshot(), vec![3, 4, 5], "вытесняются самые старые");
        assert_eq!(r.last(2), vec![4, 5]);
        assert_eq!(r.last(10), vec![3, 4, 5], "просили больше, чем есть: отдать всё, не паниковать");
    }

    #[test]
    fn recent_before_it_is_full_and_with_zero_capacity() {
        let mut r = Recent::new(4);
        r.push("x");
        r.push("y");
        assert_eq!(r.snapshot(), vec!["x", "y"]);
        let mut none = Recent::new(0);
        none.push(1);
        assert!(none.snapshot().is_empty(), "буфер на ноль элементов ничего не хранит");
    }
}
