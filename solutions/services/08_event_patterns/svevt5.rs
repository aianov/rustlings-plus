use std::collections::{HashMap, HashSet, VecDeque};

fn main() {}

pub struct Dedup {
    capacity: usize,
    order: VecDeque<String>,
    seen: HashSet<String>,
}

impl Dedup {
    pub fn new(capacity: usize) -> Dedup {
        Dedup { capacity, order: VecDeque::new(), seen: HashSet::new() }
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    pub fn first_time(&mut self, id: &str) -> bool {
        if self.capacity == 0 {
            return true;
        }
        if self.seen.contains(id) {
            return false;
        }
        if self.order.len() == self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.seen.remove(&oldest);
        }
        self.order.push_back(id.to_string());
        self.seen.insert(id.to_string());
        true
    }
}

pub struct Liked {
    pub event_id: String,
    pub post_id: String,
}

pub struct LikeCounter {
    dedup: Dedup,
    likes: HashMap<String, u64>,
}

impl LikeCounter {
    pub fn new(capacity: usize) -> LikeCounter {
        LikeCounter { dedup: Dedup::new(capacity), likes: HashMap::new() }
    }

    pub fn on_liked(&mut self, event: &Liked) -> bool {
        if !self.dedup.first_time(&event.event_id) {
            return false;
        }
        *self.likes.entry(event.post_id.clone()).or_default() += 1;
        true
    }

    pub fn count(&self, post_id: &str) -> u64 {
        self.likes.get(post_id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liked(id: &str, post: &str) -> Liked {
        Liked { event_id: id.to_string(), post_id: post.to_string() }
    }

    #[test]
    fn a_repeated_event_is_counted_once() {
        let mut counter = LikeCounter::new(100);
        assert!(counter.on_liked(&liked("e1", "p1")), "первый раз событие новое");
        assert!(!counter.on_liked(&liked("e1", "p1")), "второй раз это повтор");
        assert!(counter.on_liked(&liked("e2", "p1")), "другое событие про тот же пост это новый лайк");
        assert_eq!(counter.count("p1"), 2, "два разных события, повтор не в счёт");
        assert_eq!(counter.count("p2"), 0, "про чужой пост ничего не известно");
    }

    #[test]
    fn memory_is_bounded() {
        let mut dedup = Dedup::new(50);
        for i in 0..10_000 {
            assert!(dedup.first_time(&format!("e{i}")), "все идентификаторы разные");
        }
        assert_eq!(dedup.len(), 50, "помним ровно capacity последних, а не всё подряд");
    }

    #[test]
    fn the_oldest_is_forgotten_first_and_a_repeat_does_not_renew_it() {
        let mut dedup = Dedup::new(3);
        for id in ["a", "b", "c"] {
            assert!(dedup.first_time(id));
        }
        assert!(dedup.first_time("d"), "d новое, a вытеснено");
        assert!(!dedup.first_time("b"), "b ещё в окне");
        assert!(dedup.first_time("e"), "e новое, вытесняется самое старое по первому появлению: b");
        assert!(dedup.first_time("b"), "повтор b не продлевал ему жизнь, поэтому b уже забыт");
        assert!(dedup.first_time("a"), "и a давно забыт");
    }

    #[test]
    fn what_fell_out_of_the_window_is_counted_again() {
        let mut counter = LikeCounter::new(2);
        counter.on_liked(&liked("e1", "p1"));
        counter.on_liked(&liked("e2", "p1"));
        counter.on_liked(&liked("e3", "p1"));
        assert!(counter.on_liked(&liked("e1", "p1")), "e1 давно вне окна: за такую цену мы платим за ограниченную память");
        assert_eq!(counter.count("p1"), 4, "известное ограничение, его надо знать");
    }

    #[test]
    fn a_zero_window_remembers_nothing() {
        let mut dedup = Dedup::new(0);
        assert!(dedup.first_time("a") && dedup.first_time("a"), "окно нулевое: повторов не узнаём");
        assert!(dedup.is_empty(), "и ничего не храним");
    }
}
