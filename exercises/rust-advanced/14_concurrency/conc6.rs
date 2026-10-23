// Журнал событий сервера лежит под `Mutex`. Замок надо держать как можно короче: пока один поток формирует
// строку (а это может быть дорого, или вообще вызов чужого кода), остальные стоят. Второй сюжет:
// поток упал, держа замок, и замок «отравился». Данные при этом никуда не делись.
// TODO: Допиши методы. `add_with` обязан вызывать `make` вне замка. А `recover` и `push_anyway`
// должны работать и с отравленным замком.
use std::sync::{Mutex, PoisonError};

pub struct Journal {
    lines: Mutex<Vec<String>>,
}

impl Journal {
    pub fn new() -> Self {
        todo!("пустой журнал")
    }

    pub fn add(&self, line: String) {
        todo!("добавить строку")
    }

    pub fn add_with(&self, make: impl FnOnce() -> String) {
        todo!("сформировать строку и добавить; make вызывается без захваченного замка")
    }

    pub fn snapshot(&self) -> Vec<String> {
        todo!("копия строк на сейчас")
    }
}

pub fn recover(m: &Mutex<Vec<i32>>) -> Vec<i32> {
    todo!("прочитать данные, даже если замок отравлен")
}

pub fn push_anyway(m: &Mutex<Vec<i32>>, value: i32) {
    todo!("добавить число, даже если замок отравлен")
}

impl Default for Journal {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {
    let journal = Journal::new();
    journal.add("старт".to_string());
    println!("{:?}", journal.snapshot());
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
    fn lines_are_kept_in_order() {
        let journal = Journal::new();
        journal.add("a".into());
        journal.add_with(|| "b".into());
        assert_eq!(journal.snapshot(), vec!["a", "b"]);
    }

    #[test]
    fn making_a_line_does_not_hold_the_lock() {
        within(2, "add_with", || {
            let journal = Arc::new(Journal::new());
            journal.add("раньше".into());
            let seen = Arc::clone(&journal);
            journal.add_with(move || format!("видно строк: {}", seen.snapshot().len()));
            assert_eq!(journal.snapshot(), vec!["раньше", "видно строк: 1"]);
        });
    }

    #[test]
    fn many_threads_do_not_lose_lines() {
        let journal = Arc::new(Journal::new());
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let journal = Arc::clone(&journal);
                thread::spawn(move || {
                    for k in 0..100 {
                        journal.add(format!("{i}:{k}"));
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(journal.snapshot().len(), 800);
    }

    fn poisoned() -> Arc<Mutex<Vec<i32>>> {
        let m = Arc::new(Mutex::new(vec![1, 2]));
        let held = Arc::clone(&m);
        let _ = thread::spawn(move || {
            let mut guard = held.lock().unwrap();
            guard.push(3);
            panic!("поток упал под замком");
        })
        .join();
        assert!(m.is_poisoned(), "замок должен быть отравлен");
        m
    }

    #[test]
    fn data_survives_a_poisoned_lock() {
        let m = poisoned();
        assert_eq!(recover(&m), vec![1, 2, 3], "данные, которые поток успел записать, остаются");
        push_anyway(&m, 4);
        assert_eq!(recover(&m), vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_healthy_lock_works_the_same_way() {
        let m = Mutex::new(vec![5]);
        push_anyway(&m, 6);
        assert_eq!(recover(&m), vec![5, 6]);
    }
}
