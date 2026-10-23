use std::thread;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub fn log_from_threads(users: &[&str]) -> Vec<String> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for user in users {
        let log = Arc::clone(&log);
        let user = user.to_string();
        handles.push(thread::spawn(move || {
            log.lock().unwrap().push(format!("вход: {user}"));
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    Arc::try_unwrap(log).expect("потоки закончились, других владельцев нет").into_inner().unwrap()
}

pub fn count_visits(threads: usize, per_thread: u64) -> u64 {
    let total = Arc::new(AtomicU64::new(0));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let total = Arc::clone(&total);
            thread::spawn(move || {
                for _ in 0..per_thread {
                    total.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    total.load(Ordering::Relaxed)
}

fn main() {
    println!("{:?}", log_from_threads(&["ann", "bob"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_user_lands_in_the_log() {
        let names: Vec<String> = (0..40).map(|i| format!("user{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut log = log_from_threads(&refs);
        assert_eq!(log.len(), 40, "в журнале должна быть запись от каждого потока");
        log.sort();
        let mut expected: Vec<String> = names.iter().map(|n| format!("вход: {n}")).collect();
        expected.sort();
        assert_eq!(log, expected);
    }

    #[test]
    fn empty_input_gives_an_empty_log() {
        assert!(log_from_threads(&[]).is_empty());
    }

    #[test]
    fn visits_are_counted_without_loss() {
        assert_eq!(count_visits(8, 5000), 40_000, "обновления счётчика из разных потоков не должны теряться");
        assert_eq!(count_visits(0, 10), 0);
    }
}
