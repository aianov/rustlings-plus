// Сессии живут десять минут, и проверить это настоящими часами значит ждать десять минут. Время и номера
// нужно получать снаружи: сервис берёт `Clock` и `IdGen`, а тест подставляет часы, которыми управляет сам,
// и генератор, который выдаёт предсказуемые номера. Напиши подставные `FixedClock` и `SeqIds`
// и сам сервис `Sessions`: он выдаёт номер, запоминает время истечения и умеет чистить просроченное.
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

pub struct FixedClock {}

impl FixedClock {
    pub fn new(now: u64) -> FixedClock {
        todo!("часы, стоящие на моменте now")
    }

    pub fn advance(&self, secs: u64) {
        todo!("сдвинуть время вперёд")
    }

    pub fn set(&self, now: u64) {
        todo!("поставить время")
    }
}

impl Clock for FixedClock {
    fn now(&self) -> u64 {
        todo!()
    }
}

pub struct SeqIds {}

impl SeqIds {
    pub fn starting_at(first: u64) -> SeqIds {
        todo!("генератор: first, first + 1, ...")
    }
}

impl IdGen for SeqIds {
    fn next_id(&self) -> u64 {
        todo!()
    }
}

pub struct Sessions<C: Clock, I: IdGen> {
    clock: C,
    ids: I,
    ttl: u64,
}

impl<C: Clock, I: IdGen> Sessions<C, I> {
    pub fn new(clock: C, ids: I, ttl: u64) -> Self {
        todo!()
    }

    pub fn open(&self) -> u64 {
        todo!("выдать номер и запомнить, до какого момента сессия жива")
    }

    pub fn is_valid(&self, id: u64) -> bool {
        todo!("жива, пока текущее время меньше времени истечения")
    }

    pub fn purge(&self) -> usize {
        todo!("удалить просроченные и вернуть, сколько удалено")
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
