// Рабочий поток ждёт, пока в очереди что-то появится, а ещё группа потоков ждёт сигнала старта. Спать
// в цикле с `sleep` и проверять флаг нельзя: либо тратим процессор, либо реагируем с задержкой.
// Для этого есть `Condvar`: поток засыпает до уведомления и не нагружает систему.
// TODO: Допиши `Gate` и `Mailbox`. Ожидание условия пиши как `wait_while` (или цикл `while` с `wait`),
// слово `sleep` в коде встречаться не должно.
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

pub struct Mailbox<T> {
    items: Mutex<VecDeque<T>>,
    arrived: Condvar,
}

impl Gate {
    pub fn new() -> Self {
        todo!("закрытые ворота")
    }

    pub fn open(&self) {
        todo!("открыть и разбудить всех ждущих")
    }

    pub fn wait_open(&self) {
        todo!("спать, пока ворота закрыты")
    }
}

impl<T> Mailbox<T> {
    pub fn new() -> Self {
        todo!("пустой ящик")
    }

    pub fn push(&self, item: T) {
        todo!("положить и разбудить ждущего")
    }

    pub fn pop(&self) -> T {
        todo!("взять первое, а если пусто, спать до появления")
    }

    pub fn pop_timeout(&self, wait: Duration) -> Option<T> {
        todo!("то же, но не дольше wait; по истечении None")
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Default for Mailbox<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {
    let gate = Gate::new();
    gate.open();
    gate.wait_open();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    const SRC: &str = include_str!("conc11.rs");
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
    fn the_code_waits_on_a_condvar_instead_of_polling() {
        let code = code_only();
        assert!(!code.contains("sleep"), "ждать нужно через Condvar, а не усыпляя поток в цикле");
        assert!(code.contains(".wait"), "ожидание должно идти через wait/wait_while/wait_timeout");
    }

    #[test]
    fn every_waiter_is_released_by_open() {
        let passed = within(5, "ворота", || {
            let gate = Arc::new(Gate::new());
            let passed = Arc::new(AtomicUsize::new(0));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let gate = Arc::clone(&gate);
                    let passed = Arc::clone(&passed);
                    thread::spawn(move || {
                        gate.wait_open();
                        passed.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .collect();
            thread::sleep(Duration::from_millis(80));
            assert_eq!(passed.load(Ordering::SeqCst), 0, "пока ворота закрыты, никто не проходит");
            gate.open();
            for handle in handles {
                handle.join().unwrap();
            }
            passed.load(Ordering::SeqCst)
        });
        assert_eq!(passed, 4, "open должен будить всех, а не одного");
    }

    #[test]
    fn open_gate_lets_late_comers_through() {
        within(2, "открытые ворота", || {
            let gate = Gate::new();
            gate.open();
            gate.wait_open();
        });
    }

    #[test]
    fn pop_waits_for_a_push_from_another_thread() {
        let got = within(5, "pop", || {
            let mailbox = Arc::new(Mailbox::new());
            let producer = Arc::clone(&mailbox);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(60));
                producer.push("привет".to_string());
            });
            mailbox.pop()
        });
        assert_eq!(got, "привет");
    }

    #[test]
    fn items_come_out_in_order() {
        within(2, "порядок", || {
            let mailbox = Mailbox::new();
            mailbox.push(1);
            mailbox.push(2);
            mailbox.push(3);
            assert_eq!((mailbox.pop(), mailbox.pop(), mailbox.pop()), (1, 2, 3));
        });
    }

    #[test]
    fn pop_timeout_gives_up_on_time() {
        within(3, "pop_timeout", || {
            let mailbox: Mailbox<u8> = Mailbox::new();
            let started = std::time::Instant::now();
            assert_eq!(mailbox.pop_timeout(Duration::from_millis(50)), None);
            assert!(started.elapsed() >= Duration::from_millis(40), "таймаут не должен срабатывать раньше срока");
            mailbox.push(5);
            assert_eq!(mailbox.pop_timeout(Duration::from_millis(50)), Some(5));
        });
    }
}
