use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub fn new(value: T) -> Self {
        SpinLock { locked: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }

    pub fn lock(&self) -> SpinGuard<'_, T> {
        while self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            std::hint::spin_loop();
        }
        SpinGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<SpinGuard<'_, T>> {
        self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).ok().map(|_| SpinGuard { lock: self })
    }
}

// SAFETY: доступ к значению даёт только охранник, а он существует у одного потока за раз, поэтому
// общий доступ к замку безопасен, когда само значение можно передавать между потоками.
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> std::ops::Deref for SpinGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: охранник жив, значит замок удерживается этим потоком и других ссылок на значение нет.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> std::ops::DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: замок удерживается, а &mut self гарантирует единственность охранника.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

fn main() {
    let lock = SpinLock::new(0);
    *lock.lock() += 1;
    println!("{}", *lock.lock());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    const SRC: &str = include_str!("uns7.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }

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

    fn compiles(extra: &str) -> bool {
        compiles_with_lints(extra, &[])
    }

    fn code_only() -> String {
        SRC.split("#[cfg(test)]").next().unwrap_or("").lines().map(|line| line.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn guard_gives_access_and_unlocks_on_drop() {
        let lock = SpinLock::new(vec![1]);
        {
            let mut guard = lock.lock();
            guard.push(2);
            assert_eq!(guard.len(), 2);
        }
        assert_eq!(*lock.lock(), vec![1, 2]);
    }

    #[test]
    fn try_lock_fails_while_the_lock_is_held() {
        let lock = SpinLock::new(0);
        let guard = lock.try_lock().expect("свободный замок должен захватываться");
        assert!(lock.try_lock().is_none(), "пока охранник жив, второй захват невозможен");
        drop(guard);
        assert!(lock.try_lock().is_some(), "после drop замок свободен");
    }

    #[test]
    fn four_threads_never_lose_an_increment() {
        let total = within(10, "спинлок", || {
            let lock = Arc::new(SpinLock::new(0_u64));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let lock = Arc::clone(&lock);
                    thread::spawn(move || {
                        for _ in 0..10_000 {
                            let mut guard = lock.lock();
                            let current = *guard;
                            thread::yield_now();
                            *guard = current + 1;
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            *lock.lock()
        });
        assert_eq!(total, 40_000, "замок не обеспечил взаимное исключение");
    }

    #[test]
    fn the_lock_is_sync_only_for_sendable_contents() {
        assert!(compiles("fn need<T: Sync>() {}\nfn probe() { need::<SpinLock<Vec<String>>>(); }"), "SpinLock<Vec<_>> должен быть Sync");
        assert!(compiles("fn need<T: Sync>() {}\nfn probe() { need::<SpinLock<std::cell::Cell<u8>>>(); }"), "SpinLock<Cell<_>> должен быть Sync, как Mutex");
        assert!(
            !compiles("fn need<T: Sync>() {}\nfn probe() { need::<SpinLock<std::rc::Rc<u8>>>(); }"),
            "SpinLock<Rc<_>> не должен быть Sync: Rc нельзя пускать в другой поток"
        );
    }

    #[test]
    fn orderings_pair_acquire_with_release() {
        let code = code_only();
        assert!(code.contains("Acquire"), "захват флага должен быть с Acquire");
        assert!(code.contains("Release"), "освобождение флага должно быть с Release");
        assert!(!code.contains("SeqCst"), "SeqCst тут не нужен");
        assert!(code.contains("unsafe impl"), "замок разделяется между потоками только через unsafe impl Sync");
    }
}
