use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock {
    fn now(&self) -> u64;
}

pub trait IdGen {
    fn next_id(&self) -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
}

pub struct FixedClock {
    now: AtomicU64,
}

impl FixedClock {
    pub fn new(now: u64) -> FixedClock {
        FixedClock { now: AtomicU64::new(now) }
    }

    pub fn advance(&self, secs: u64) {
        self.now.fetch_add(secs, Ordering::SeqCst);
    }

    pub fn set(&self, now: u64) {
        self.now.store(now, Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

pub struct SeqIds {
    next: AtomicU64,
}

impl SeqIds {
    pub fn starting_at(first: u64) -> SeqIds {
        SeqIds { next: AtomicU64::new(first) }
    }
}

impl IdGen for SeqIds {
    fn next_id(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst)
    }
}

pub struct Sessions<C: Clock, I: IdGen> {
    clock: C,
    ids: I,
    ttl: u64,
    expires: Mutex<HashMap<u64, u64>>,
}

impl<C: Clock, I: IdGen> Sessions<C, I> {
    pub fn new(clock: C, ids: I, ttl: u64) -> Self {
        Sessions { clock, ids, ttl, expires: Mutex::new(HashMap::new()) }
    }

    pub fn open(&self) -> u64 {
        let id = self.ids.next_id();
        let until = self.clock.now() + self.ttl;
        self.expires.lock().expect("замок сессий отравлен").insert(id, until);
        id
    }

    pub fn is_valid(&self, id: u64) -> bool {
        let now = self.clock.now();
        self.expires.lock().expect("замок сессий отравлен").get(&id).is_some_and(|&until| now < until)
    }

    pub fn purge(&self) -> usize {
        let now = self.clock.now();
        let mut map = self.expires.lock().expect("замок сессий отравлен");
        let before = map.len();
        map.retain(|_, until| now < *until);
        before - map.len()
    }
}

fn main() {
    let sessions = Sessions::new(SystemClock, SeqIds::starting_at(1), 600);
    println!("{}", sessions.open());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sessions(start: u64) -> Sessions<FixedClock, SeqIds> {
        Sessions::new(FixedClock::new(start), SeqIds::starting_at(100), 600)
    }

    #[test]
    fn ids_are_predictable() {
        let ids = SeqIds::starting_at(41);
        assert_eq!((ids.next_id(), ids.next_id(), ids.next_id()), (41, 42, 43), "номера идут подряд от заданного");
    }

    #[test]
    fn fixed_clock_moves_only_when_told() {
        let clock = FixedClock::new(1_000);
        assert_eq!(clock.now(), 1_000);
        assert_eq!(clock.now(), 1_000, "часы не идут сами");
        clock.advance(30);
        assert_eq!(clock.now(), 1_030);
        clock.set(5);
        assert_eq!(clock.now(), 5);
    }

    #[test]
    fn session_lives_exactly_ttl_seconds() {
        let s = sessions(10_000);
        let id = s.open();
        assert_eq!(id, 100, "первый номер из генератора");
        s.clock.advance(599);
        assert!(s.is_valid(id), "за секунду до истечения сессия жива");
        s.clock.advance(1);
        assert!(!s.is_valid(id), "ровно через ttl секунд сессия уже просрочена");
    }

    #[test]
    fn unknown_session_is_not_valid() {
        assert!(!sessions(0).is_valid(12345));
    }

    #[test]
    fn purge_removes_only_expired() {
        let s = sessions(0);
        let old = s.open();
        s.clock.advance(400);
        let fresh = s.open();
        s.clock.advance(250);
        assert_eq!(s.purge(), 1, "просрочена только первая сессия");
        assert!(!s.is_valid(old));
        assert!(s.is_valid(fresh));
        assert_eq!(s.purge(), 0, "повторная чистка ничего не находит");
    }

    #[test]
    fn expiry_is_counted_from_opening_moment() {
        let s = sessions(5_000);
        s.clock.advance(100);
        let id = s.open();
        s.clock.advance(599);
        assert!(s.is_valid(id), "срок считается от момента открытия, а не от старта часов");
    }
}
