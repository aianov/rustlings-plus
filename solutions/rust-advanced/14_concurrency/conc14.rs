use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(test)]
use std::thread;

#[derive(Default)]
pub struct Counters {
    hits: AtomicU64,
    max_seen: AtomicU64,
    slots: AtomicUsize,
}

impl Counters {
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub fn max_seen(&self) -> u64 {
        self.max_seen.load(Ordering::Relaxed)
    }

    pub fn hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe(&self, value: u64) {
        self.max_seen.fetch_max(value, Ordering::Relaxed);
    }

    pub fn take_slot(&self, limit: usize) -> Option<usize> {
        self.slots.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |taken| (taken < limit).then_some(taken + 1)).ok()
    }
}

fn main() {
    let counters = Counters::default();
    counters.hit();
    println!("{}", counters.hits());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[test]
    fn hits_are_not_lost() {
        let counters = Arc::new(Counters::default());
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let counters = Arc::clone(&counters);
                thread::spawn(move || {
                    for _ in 0..2000 {
                        counters.hit();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(counters.hits(), 16_000, "часть обращений потерялась: чтение и запись идут двумя шагами");
    }

    #[test]
    fn the_maximum_never_goes_back() {
        let counters = Arc::new(Counters::default());
        let handles: Vec<_> = (0..8u64)
            .map(|i| {
                let counters = Arc::clone(&counters);
                thread::spawn(move || {
                    for k in 0..2000 {
                        counters.observe(i * 2000 + k);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(counters.max_seen(), 15_999, "максимум откатился: старое значение записали поверх нового");
    }

    #[test]
    fn slots_are_handed_out_exactly_once() {
        let counters = Arc::new(Counters::default());
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let counters = Arc::clone(&counters);
                thread::spawn(move || {
                    let mut mine = Vec::new();
                    while let Some(slot) = counters.take_slot(100) {
                        mine.push(slot);
                    }
                    mine
                })
            })
            .collect();
        let all: Vec<usize> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        let unique: HashSet<_> = all.iter().copied().collect();
        assert_eq!(all.len(), 100, "слотов выдано не {} из 100: лимит нарушен или слот потерян", all.len());
        assert_eq!(unique.len(), 100, "один слот достался двум потокам");
        assert_eq!(counters.take_slot(100), None);
    }

    #[test]
    fn a_zero_limit_gives_nothing() {
        assert_eq!(Counters::default().take_slot(0), None);
    }
}
