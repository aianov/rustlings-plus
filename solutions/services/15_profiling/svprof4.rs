
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

fn main() {}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LockStats {
    pub acquisitions: u64,
    pub contended: u64,
    pub total_wait: Duration,
    pub max_wait: Duration,
    pub total_hold: Duration,
    pub max_hold: Duration,
}

pub struct TimedMutex<T> {
    inner: Mutex<T>,
    stats: Mutex<LockStats>,
}

pub struct TimedGuard<'a, T> {
    guard: Option<MutexGuard<'a, T>>,
    owner: &'a TimedMutex<T>,
    acquired: Instant,
}

impl<T> TimedMutex<T> {
    pub fn new(value: T) -> TimedMutex<T> {
        TimedMutex { inner: Mutex::new(value), stats: Mutex::new(LockStats::default()) }
    }

    pub fn lock(&self) -> TimedGuard<'_, T> {
        let attempt = Instant::now();
        let (guard, waited) = match self.inner.try_lock() {
            Ok(guard) => (guard, None),
            Err(TryLockError::Poisoned(e)) => (e.into_inner(), None),
            Err(TryLockError::WouldBlock) => {
                let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                (guard, Some(attempt.elapsed()))
            }
        };
        {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.acquisitions += 1;
            if let Some(wait) = waited {
                stats.contended += 1;
                stats.total_wait += wait;
                stats.max_wait = stats.max_wait.max(wait);
            }
        }
        TimedGuard { guard: Some(guard), owner: self, acquired: Instant::now() }
    }

    pub fn stats(&self) -> LockStats {
        *self.stats.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl<T> Deref for TimedGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.guard.as_ref().expect("охрана жива до Drop")
    }
}

impl<T> DerefMut for TimedGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.guard.as_mut().expect("охрана жива до Drop")
    }
}

impl<T> Drop for TimedGuard<'_, T> {
    fn drop(&mut self) {
        let held = self.acquired.elapsed();
        {
            let mut stats = self.owner.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.total_hold += held;
            stats.max_hold = stats.max_hold.max(held);
        }
        self.guard.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::mpsc;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn it_still_protects_the_value() {
        let m = TimedMutex::new(Vec::<u32>::new());
        m.lock().push(1);
        m.lock().push(2);
        assert_eq!(*m.lock(), vec![1, 2]);
    }

    #[test]
    fn uncontended_locks_are_counted_with_zero_wait_and_real_hold_time() {
        let m = TimedMutex::new(0u32);
        for _ in 0..3 {
            let mut g = m.lock();
            *g += 1;
            std::thread::sleep(MS(10));
        }
        let s = m.stats();
        assert_eq!((s.acquisitions, s.contended), (3, 0), "{s:?}");
        assert_eq!((s.total_wait, s.max_wait), (Duration::ZERO, Duration::ZERO), "свободный замок берётся без ожидания");
        assert!(s.total_hold >= MS(30) && s.max_hold >= MS(10), "удержание считается от получения до отпускания: {s:?}");
    }

    #[test]
    fn a_busy_lock_makes_the_second_thread_wait_and_the_wait_is_measured() {
        let m = Arc::new(TimedMutex::new(0u32));
        let (tx, rx) = mpsc::channel();
        let holder = {
            let m = m.clone();
            std::thread::spawn(move || {
                let _g = m.lock();
                tx.send(()).expect("главный поток ждёт");
                std::thread::sleep(MS(120));
            })
        };
        rx.recv().expect("замок взят");
        drop(m.lock());
        holder.join().expect("поток не упал");
        let s = m.stats();
        assert_eq!(s.acquisitions, 2);
        assert_eq!(s.contended, 1, "второй захват застал замок занятым: {s:?}");
        assert!(s.max_wait >= MS(80), "ждали около 120 мс: {:?}", s.max_wait);
        assert_eq!(s.total_wait, s.max_wait, "ожидал один захват");
        assert!(s.max_hold >= MS(120), "первый поток держал замок 120 мс: {:?}", s.max_hold);
    }

    #[test]
    fn waiting_is_not_counted_as_holding() {
        let m = Arc::new(TimedMutex::new(()));
        let (tx, rx) = mpsc::channel();
        let holder = {
            let m = m.clone();
            std::thread::spawn(move || {
                let _g = m.lock();
                tx.send(()).expect("главный поток ждёт");
                std::thread::sleep(MS(100));
            })
        };
        rx.recv().expect("замок взят");
        drop(m.lock());
        holder.join().expect("поток не упал");
        let s = m.stats();
        assert!(s.total_hold < MS(180), "общее удержание {:?}: сто мс держал первый, второй взял и сразу отпустил; ожидание второго сюда входить не должно", s.total_hold);
    }

    #[test]
    fn a_thread_that_panics_while_holding_does_not_break_the_lock_or_the_accounting() {
        let m = Arc::new(TimedMutex::new(5u32));
        let m2 = m.clone();
        let crashed = std::thread::spawn(move || {
            let _g = m2.lock();
            panic!("упали, держа замок");
        })
        .join();
        assert!(crashed.is_err());
        assert_eq!(*m.lock(), 5, "отравление паникой не должно делать замок непригодным");
        let s = m.stats();
        assert_eq!(s.acquisitions, 2, "{s:?}");
        assert!(s.max_hold > Duration::ZERO, "удержание упавшего потока записано при раскрутке стека");
    }
}
