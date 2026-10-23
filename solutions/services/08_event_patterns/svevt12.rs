use std::collections::{BTreeMap, HashMap};

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct Versioned {
    pub aggregate: String,
    pub version: u64,
    pub body: String,
}

#[derive(Debug, PartialEq)]
pub enum SeqError {
    TooFarAhead,
}

pub struct Sequencer {
    next: HashMap<String, u64>,
    held: HashMap<String, BTreeMap<u64, Versioned>>,
    max_held: usize,
}

impl Sequencer {
    pub fn new(max_held: usize) -> Sequencer {
        Sequencer { next: HashMap::new(), held: HashMap::new(), max_held }
    }

    pub fn waiting(&self, aggregate: &str) -> usize {
        self.held.get(aggregate).map_or(0, BTreeMap::len)
    }

    pub fn offer(&mut self, event: Versioned) -> Result<Vec<Versioned>, SeqError> {
        let expected = *self.next.get(&event.aggregate).unwrap_or(&1);
        if event.version < expected {
            return Ok(Vec::new());
        }
        let held = self.held.entry(event.aggregate.clone()).or_default();
        if event.version > expected {
            if held.contains_key(&event.version) {
                return Ok(Vec::new());
            }
            if held.len() >= self.max_held {
                return Err(SeqError::TooFarAhead);
            }
            held.insert(event.version, event);
            return Ok(Vec::new());
        }
        let aggregate = event.aggregate.clone();
        let mut ready = vec![event];
        let mut next = expected + 1;
        while let Some(following) = held.remove(&next) {
            ready.push(following);
            next += 1;
        }
        self.next.insert(aggregate, next);
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(aggregate: &str, version: u64) -> Versioned {
        Versioned { aggregate: aggregate.into(), version, body: format!("{aggregate}#{version}") }
    }

    fn versions(events: Vec<Versioned>) -> Vec<u64> {
        events.into_iter().map(|e| e.version).collect()
    }

    #[test]
    fn events_in_order_pass_straight_through() {
        let mut seq = Sequencer::new(10);
        for v in 1..=3 {
            assert_eq!(versions(seq.offer(ev("u1", v)).unwrap()), vec![v], "версия {v} пришла вовремя");
        }
    }

    #[test]
    fn a_late_event_releases_the_waiting_ones() {
        let mut seq = Sequencer::new(10);
        assert!(seq.offer(ev("u1", 3)).unwrap().is_empty(), "тройка ждёт");
        assert!(seq.offer(ev("u1", 2)).unwrap().is_empty(), "двойка тоже ждёт, единицы нет");
        assert_eq!(seq.waiting("u1"), 2, "двое в очереди");
        assert_eq!(versions(seq.offer(ev("u1", 1)).unwrap()), vec![1, 2, 3], "единица открывает весь хвост, строго по порядку");
        assert_eq!(seq.waiting("u1"), 0, "очередь пуста");
        assert_eq!(versions(seq.offer(ev("u1", 4)).unwrap()), vec![4], "дальше всё по-прежнему");
    }

    #[test]
    fn only_the_contiguous_part_is_released() {
        let mut seq = Sequencer::new(10);
        seq.offer(ev("u1", 2)).unwrap();
        seq.offer(ev("u1", 4)).unwrap();
        assert_eq!(versions(seq.offer(ev("u1", 1)).unwrap()), vec![1, 2], "тройки нет, четвёрка остаётся ждать");
        assert_eq!(seq.waiting("u1"), 1, "четвёрка в очереди");
        assert_eq!(versions(seq.offer(ev("u1", 3)).unwrap()), vec![3, 4], "теперь и она");
    }

    #[test]
    fn repeats_are_ignored_whether_applied_or_waiting() {
        let mut seq = Sequencer::new(10);
        seq.offer(ev("u1", 1)).unwrap();
        assert!(seq.offer(ev("u1", 1)).unwrap().is_empty(), "версия 1 уже применена");
        seq.offer(ev("u1", 3)).unwrap();
        assert!(seq.offer(ev("u1", 3)).unwrap().is_empty(), "версия 3 уже ждёт");
        assert_eq!(seq.waiting("u1"), 1, "ожидающая одна, не две");
        assert_eq!(versions(seq.offer(ev("u1", 2)).unwrap()), vec![2, 3], "и выдана она один раз");
    }

    #[test]
    fn aggregates_do_not_wait_for_each_other() {
        let mut seq = Sequencer::new(10);
        seq.offer(ev("u1", 2)).unwrap();
        assert_eq!(versions(seq.offer(ev("u2", 1)).unwrap()), vec![1], "u2 не ждёт отставшего u1");
        assert_eq!(seq.waiting("u1"), 1, "u1 по-прежнему ждёт единицу");
        assert_eq!(seq.waiting("u2"), 0, "у u2 очереди нет");
    }

    #[test]
    fn the_waiting_queue_is_bounded() {
        let mut seq = Sequencer::new(2);
        seq.offer(ev("u1", 5)).unwrap();
        seq.offer(ev("u1", 6)).unwrap();
        assert_eq!(seq.offer(ev("u1", 7)), Err(SeqError::TooFarAhead), "третье ожидающее не помещается");
        assert!(seq.offer(ev("u2", 9)).is_ok(), "лимит у каждого агрегата свой");
        assert_eq!(seq.waiting("u1"), 2, "память не выросла");
    }
}
