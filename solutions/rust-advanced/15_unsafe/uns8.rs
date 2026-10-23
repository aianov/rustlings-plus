use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Shared<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: значением владеет замок, одновременно к нему обращается один поток.
unsafe impl<T: Send> Sync for Shared<T> {}

struct Unlock<'a>(&'a AtomicBool);

impl Drop for Unlock<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Shared { locked: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    fn lock(&self) -> Unlock<'_> {
        while self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            std::hint::spin_loop();
        }
        Unlock(&self.locked)
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let _unlock = self.lock();
        // SAFETY: замок удерживается до конца функции, других ссылок на значение нет.
        f(unsafe { &mut *self.value.get() })
    }

    pub fn try_update<R, E>(&self, f: impl FnOnce(&mut T) -> Result<R, E>) -> Result<R, E> {
        let _unlock = self.lock();
        // SAFETY: замок удерживается до конца функции, других ссылок на значение нет.
        f(unsafe { &mut *self.value.get() })
    }
}

fn main() {
    let shared = Shared::new(1);
    println!("{}", shared.update(|v| *v + 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
        }
    }

    #[test]
    fn the_happy_path_works() {
        within(2, "счастливый путь", || {
            let shared = Shared::new(vec![1]);
            let len = shared.update(|v| {
                v.push(2);
                v.len()
            });
            assert_eq!(len, 2);
            let parsed: Result<i32, String> = shared.try_update(|v| Ok(v.iter().sum()));
            assert_eq!(parsed, Ok(3));
            assert!(!shared.is_locked(), "после вызова замок должен быть открыт");
        });
    }

    #[test]
    fn a_panic_in_the_closure_does_not_leave_the_lock_closed() {
        let (locked, value) = within(3, "паника под замком", || {
            let shared = Arc::new(Shared::new(10));
            let worker = Arc::clone(&shared);
            let result = thread::spawn(move || worker.update(|_| panic!("упали под замком"))).join();
            assert!(result.is_err());
            let locked = shared.is_locked();
            let value = shared.update(|v| {
                *v += 1;
                *v
            });
            (locked, value)
        });
        assert!(!locked, "после паники замок остался закрытым: освобождение должно быть в Drop");
        assert_eq!(value, 11, "следующий вызов должен пройти");
    }

    #[test]
    fn an_early_error_return_opens_the_lock() {
        let (answer, locked) = within(3, "ранний выход", || {
            let shared = Shared::new(5);
            let answer: Result<(), &str> = shared.try_update(|v| {
                if *v < 10 {
                    return Err("слишком мало");
                }
                *v = 0;
                Ok(())
            });
            (answer, shared.is_locked())
        });
        assert_eq!(answer, Err("слишком мало"));
        assert!(!locked, "ранний выход оставил замок закрытым");
    }

    #[test]
    fn concurrent_updates_stay_exclusive() {
        let total = within(10, "параллельные обновления", || {
            let shared = Arc::new(Shared::new(0_u64));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let shared = Arc::clone(&shared);
                    thread::spawn(move || {
                        for _ in 0..5_000 {
                            shared.update(|v| {
                                let current = *v;
                                thread::yield_now();
                                *v = current + 1;
                            });
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            shared.update(|v| *v)
        });
        assert_eq!(total, 20_000);
    }
}
