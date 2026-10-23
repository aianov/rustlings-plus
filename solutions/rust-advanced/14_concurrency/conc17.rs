use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct MiniPool {
    sender: Option<mpsc::Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

fn work(receiver: &Mutex<Receiver<Job>>) {
    loop {
        let job = receiver.lock().unwrap().recv();
        match job {
            Ok(job) => job(),
            Err(_) => break,
        }
    }
}

impl MiniPool {
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..size)
            .map(|_| {
                let receiver = Arc::clone(&receiver);
                thread::spawn(move || work(&receiver))
            })
            .collect();
        MiniPool { sender: Some(sender), workers }
    }

    pub fn execute(&self, job: impl FnOnce() + Send + 'static) {
        self.sender.as_ref().expect("пул остановлен").send(Box::new(job)).expect("рабочие живы");
    }
}

impl Drop for MiniPool {
    fn drop(&mut self) {
        drop(self.sender.take());
        for worker in self.workers.drain(..) {
            worker.join().unwrap();
        }
    }
}

fn main() {
    let pool = MiniPool::new(2);
    pool.execute(|| println!("привет из пула"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

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
    fn parallel_jobs_really_overlap() {
        let (tx, rx) = mpsc::channel();
        let pool = MiniPool::new(3);
        let started = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let started = Arc::clone(&started);
            let tx = tx.clone();
            pool.execute(move || {
                started.fetch_add(1, Ordering::SeqCst);
                let deadline = Instant::now() + Duration::from_secs(2);
                while started.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(1));
                }
                let _ = tx.send(started.load(Ordering::SeqCst) == 3);
            });
        }
        drop(tx);
        let mut met = 0;
        for _ in 0..3 {
            if rx.recv_timeout(Duration::from_secs(4)).unwrap_or(false) {
                met += 1;
            }
        }
        std::mem::forget(pool);
        assert_eq!(met, 3, "три рабочих должны взять три задания одновременно, а они идут по очереди");
    }

    #[test]
    fn dropping_the_pool_ends_and_finishes_the_queue() {
        let count = within(3, "drop пула", || {
            let count = Arc::new(AtomicUsize::new(0));
            {
                let pool = MiniPool::new(2);
                for _ in 0..10 {
                    let count = Arc::clone(&count);
                    pool.execute(move || {
                        thread::sleep(Duration::from_millis(5));
                        count.fetch_add(1, Ordering::SeqCst);
                    });
                }
            }
            count.load(Ordering::SeqCst)
        });
        assert_eq!(count, 10, "drop должен дождаться всех принятых заданий");
    }

    #[test]
    fn an_empty_pool_stops_too() {
        within(3, "пустой пул", || drop(MiniPool::new(3)));
    }
}
