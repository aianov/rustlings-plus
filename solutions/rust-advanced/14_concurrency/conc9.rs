use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    Invalid,
    Taken,
}

#[derive(Default)]
pub struct Users {
    by_login: Mutex<HashMap<String, u32>>,
}

impl Users {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self) -> usize {
        self.by_login.lock().unwrap().len()
    }

    pub fn register(&self, login: &str) -> Result<u32, RegisterError> {
        self.register_with(login, |login| !login.is_empty() && login.chars().all(|c| c.is_alphanumeric() || c == '_'))
    }

    pub fn register_with(&self, login: &str, validate: impl Fn(&str) -> bool) -> Result<u32, RegisterError> {
        if !validate(login) {
            return Err(RegisterError::Invalid);
        }
        let mut users = self.by_login.lock().unwrap();
        if users.contains_key(login) {
            return Err(RegisterError::Taken);
        }
        let id = users.len() as u32 + 1;
        users.insert(login.to_string(), id);
        Ok(id)
    }
}

fn main() {
    let users = Users::new();
    println!("{:?}", users.register("ann"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

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
    fn sequential_registration_works() {
        let users = Users::new();
        assert_eq!(users.register("ann"), Ok(1));
        assert_eq!(users.register("bob"), Ok(2));
        assert_eq!(users.register("ann"), Err(RegisterError::Taken));
        assert_eq!(users.register("bad login"), Err(RegisterError::Invalid));
        assert_eq!(users.register(""), Err(RegisterError::Invalid));
        assert_eq!(users.count(), 2, "невалидные и занятые логины не сохраняются");
    }

    #[test]
    fn only_one_of_many_racers_gets_the_login() {
        let (winners, count) = within(10, "гонка за логин", || {
            let users = Arc::new(Users::new());
            let barrier = Arc::new(Barrier::new(8));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let users = Arc::clone(&users);
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        users.register_with("ann", |_| {
                            thread::sleep(Duration::from_millis(30));
                            true
                        })
                    })
                })
                .collect();
            let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            let winners = results.iter().filter(|r| r.is_ok()).count();
            assert!(results.iter().filter(|r| r.is_err()).all(|r| *r == Err(RegisterError::Taken)));
            (winners, users.count())
        });
        assert_eq!(winners, 1, "логин достался нескольким пользователям: проверка и вставка идут под разными захватами");
        assert_eq!(count, 1);
    }

    #[test]
    fn ids_stay_unique_under_load() {
        let ids = within(10, "много разных логинов", || {
            let users = Arc::new(Users::new());
            let handles: Vec<_> = (0..16)
                .map(|i| {
                    let users = Arc::clone(&users);
                    thread::spawn(move || users.register(&format!("user{i}")).unwrap())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
        });
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 16, "у разных пользователей разные id");
        assert_eq!(*ids.iter().max().unwrap(), 16);
    }

    #[test]
    fn slow_validation_does_not_serialize_everyone() {
        let peak = within(10, "параллельная проверка", || {
            let users = Arc::new(Users::new());
            let inside = Arc::new(AtomicUsize::new(0));
            let peak = Arc::new(AtomicUsize::new(0));
            let barrier = Arc::new(Barrier::new(4));
            let handles: Vec<_> = (0..4)
                .map(|i| {
                    let users = Arc::clone(&users);
                    let inside = Arc::clone(&inside);
                    let peak = Arc::clone(&peak);
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        users
                            .register_with(&format!("u{i}"), |_| {
                                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                                peak.fetch_max(now, Ordering::SeqCst);
                                thread::sleep(Duration::from_millis(60));
                                inside.fetch_sub(1, Ordering::SeqCst);
                                true
                            })
                            .unwrap();
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap();
            }
            peak.load(Ordering::SeqCst)
        });
        assert!(peak >= 2, "медленная проверка идёт под замком: остальные регистрации стоят и ждут");
    }
}
