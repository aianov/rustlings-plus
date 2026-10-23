// Лайки и подписки приходят в feed-service с разных экземпляров и после повторов, поэтому события одного агрегата
// могут прийти не по порядку: версия 3 раньше версии 2. А применять их нужно строго по порядку версий 1, 2, 3...
// Цель: Sequencer::offer принимает событие и возвращает те события (может быть несколько, может не быть ни одного),
// которые теперь можно применить по порядку. Событие из будущего ждёт своей очереди; уже применённые версии и
// повторы ожидающих игнорируются; агрегаты не зависят друг от друга. Ожидающих у агрегата не больше max_held:
// иначе TooFarAhead, а не бесконечный рост памяти.

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
        let _ = event;
        todo!()
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
