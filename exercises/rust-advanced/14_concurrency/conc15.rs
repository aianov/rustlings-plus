// Один поток готовит отчёт из нескольких чисел и поднимает флаг «готово», другой ждёт флага и читает числа.
// Если флаг подняли «где-то там» относительно записи чисел, читатель может увидеть флаг, но не числа.
// Нужна пара `Release` при записи флага и `Acquire` при чтении: всё, что записано до `Release`,
// видно после `Acquire`. Для обычного счётчика посещений порядок не нужен вовсе.
// TODO: Допиши методы. Данные пиши и читай с `Relaxed`, флаг публикуй и проверяй парой
// `Release`/`Acquire`, счётчик посещений на `Relaxed`. `SeqCst` не нужен нигде.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
pub struct Report {
    ready: AtomicBool,
    user: AtomicU64,
    posts: AtomicU64,
    likes: AtomicU64,
}

#[derive(Default)]
pub struct Visits {
    count: AtomicU64,
}

impl Report {
    pub fn publish(&self, user: u64, posts: u64, likes: u64) {
        todo!("записать три числа, затем поднять флаг")
    }

    pub fn read(&self) -> Option<(u64, u64, u64)> {
        todo!("None, пока флаг не поднят; иначе все три числа")
    }
}

impl Visits {
    pub fn hit(&self) {
        todo!("посещение")
    }

    pub fn total(&self) -> u64 {
        todo!("сколько всего")
    }
}

fn main() {
    let report = Report::default();
    report.publish(1, 2, 3);
    println!("{:?}", report.read());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    const SRC: &str = include_str!("conc15.rs");
    fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
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

    #[test]
    fn nothing_is_visible_before_publishing() {
        let report = Report::default();
        assert_eq!(report.read(), None);
        report.publish(7, 100, 2500);
        assert_eq!(report.read(), Some((7, 100, 2500)));
    }

    #[test]
    fn a_reader_in_another_thread_sees_a_complete_report() {
        let seen = within(10, "читатель отчёта", || {
            let mut bad = 0;
            for round in 0..300_u64 {
                let report = Arc::new(Report::default());
                let writer = Arc::clone(&report);
                let handle = thread::spawn(move || writer.publish(round, round * 2, round * 3));
                let deadline = Instant::now() + Duration::from_secs(2);
                let got = loop {
                    if let Some(values) = report.read() {
                        break values;
                    }
                    assert!(Instant::now() < deadline, "флаг так и не поднялся");
                    std::hint::spin_loop();
                };
                if got != (round, round * 2, round * 3) {
                    bad += 1;
                }
                handle.join().unwrap();
            }
            bad
        });
        assert_eq!(seen, 0, "читатель увидел флаг, но не все числа");
    }

    #[test]
    fn visits_add_up() {
        let visits = Arc::new(Visits::default());
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let visits = Arc::clone(&visits);
                thread::spawn(move || {
                    for _ in 0..1000 {
                        visits.hit();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(visits.total(), 4000);
    }

    #[test]
    fn the_flag_uses_release_and_acquire() {
        let code = code_only();
        assert!(code.contains("Release"), "флаг готовности нужно поднимать с Ordering::Release");
        assert!(code.contains("Acquire"), "флаг готовности нужно читать с Ordering::Acquire");
        assert!(!code.contains("SeqCst"), "SeqCst тут лишний: хватает пары Release/Acquire");
        assert!(code.contains("Relaxed"), "независимые данные и счётчик читаются с Relaxed");
    }
}
