// Журнал входов пишется из нескольких потоков, и сборка не проходит: `Rc<RefCell<..>>` в поток отправить
// нельзя, компилятор говорит это прямым текстом. То же самое и с `Rc<Cell<..>>` в счётчике посещений.
// TODO: Почини обе функции так, чтобы они собирались и работали. Что должно остаться в итоге,
// решай по смыслу: журнал вернётся вызывающему целиком, а счётчик отдаст число.
use std::thread;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub fn log_from_threads(users: &[&str]) -> Vec<String> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut handles = Vec::new();
    for user in users {
        let log = Rc::clone(&log);
        let user = user.to_string();
        handles.push(thread::spawn(move || {
            log.borrow_mut().push(format!("вход: {user}"));
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    Rc::try_unwrap(log).unwrap().into_inner()
}

pub fn count_visits(threads: usize, per_thread: u64) -> u64 {
    let total = Rc::new(Cell::new(0_u64));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let total = Rc::clone(&total);
            thread::spawn(move || {
                for _ in 0..per_thread {
                    total.set(total.get() + 1);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    total.get()
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
