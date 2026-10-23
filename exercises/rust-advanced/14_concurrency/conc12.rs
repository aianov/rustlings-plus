// Две классические поломки `Condvar`. Первая: `Signal` забывает, что сигнал уже был. Если `notify`
// случился раньше, чем кто-то начал ждать, ожидающий уснёт навсегда: уведомление ни к кому не дошло.
// Вторая: `Queue::pop` проверяет очередь через `if`. Два потребителя просыпаются от одного `push`,
// первый забирает значение, второй берёт из пустой очереди.
// TODO: Почини обе структуры. `Signal` одноразовый: после `notify` любой `wait` возвращается сразу.
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

pub struct Signal {
    lock: Mutex<()>,
    changed: Condvar,
}

impl Signal {
    pub fn new() -> Self {
        Signal { lock: Mutex::new(()), changed: Condvar::new() }
    }

    pub fn notify(&self) {
        self.changed.notify_all();
    }

    pub fn wait(&self) {
        let guard = self.lock.lock().unwrap();
        let _guard = self.changed.wait(guard).unwrap();
    }
}

pub struct Queue {
    items: Mutex<VecDeque<u32>>,
    arrived: Condvar,
}

impl Queue {
    pub fn new() -> Self {
        Queue { items: Mutex::new(VecDeque::new()), arrived: Condvar::new() }
    }

    pub fn push(&self, value: u32) {
        self.items.lock().unwrap().push_back(value);
        self.arrived.notify_all();
    }

    pub fn pop(&self) -> u32 {
        let mut items = self.items.lock().unwrap();
        if items.is_empty() {
            items = self.arrived.wait(items).unwrap();
        }
        items.pop_front().unwrap()
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {
    let signal = Signal::new();
    signal.notify();
    signal.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
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
    fn a_notification_sent_early_is_not_lost() {
        within(2, "wait после notify", || {
            let signal = Signal::new();
            signal.notify();
            signal.wait();
        });
    }

    #[test]
    fn a_sleeping_waiter_wakes_up() {
        within(3, "wait до notify", || {
            let signal = Arc::new(Signal::new());
            let waiter = Arc::clone(&signal);
            let handle = thread::spawn(move || waiter.wait());
            thread::sleep(Duration::from_millis(50));
            signal.notify();
            handle.join().unwrap();
        });
    }

    #[test]
    fn one_signal_wakes_all_current_and_future_waiters() {
        within(3, "много ждущих", || {
            let signal = Arc::new(Signal::new());
            let handles: Vec<_> = (0..3)
                .map(|_| {
                    let signal = Arc::clone(&signal);
                    thread::spawn(move || signal.wait())
                })
                .collect();
            thread::sleep(Duration::from_millis(50));
            signal.notify();
            for handle in handles {
                handle.join().unwrap();
            }
            signal.wait();
        });
    }

    #[test]
    fn two_consumers_woken_by_one_push_do_not_steal_from_an_empty_queue() {
        let mut got = within(5, "два потребителя", || {
            let queue = Arc::new(Queue::new());
            let consumers: Vec<_> = (0..2)
                .map(|_| {
                    let queue = Arc::clone(&queue);
                    thread::spawn(move || queue.pop())
                })
                .collect();
            thread::sleep(Duration::from_millis(100));
            queue.push(1);
            thread::sleep(Duration::from_millis(100));
            queue.push(2);
            consumers.into_iter().map(|c| c.join().expect("потребитель упал на пустой очереди")).collect::<Vec<_>>()
        });
        got.sort();
        assert_eq!(got, vec![1, 2], "каждый потребитель должен получить своё значение");
    }
}
