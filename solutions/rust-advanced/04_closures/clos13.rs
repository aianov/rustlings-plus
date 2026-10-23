use std::sync::Arc;
use std::thread;

pub type Handler = Box<dyn Fn(u32) -> u32 + Send + Sync + 'static>;

pub struct Registry {
    handlers: Vec<Handler>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry::new()
    }
}

impl Registry {
    pub fn new() -> Registry {
        Registry { handlers: Vec::new() }
    }

    pub fn add(&mut self, handler: impl Fn(u32) -> u32 + Send + Sync + 'static) {
        self.handlers.push(Box::new(handler));
    }

    pub fn run_all(self: &Arc<Self>, input: u32) -> Vec<u32> {
        let threads: Vec<_> = (0..self.handlers.len())
            .map(|index| {
                let registry = Arc::clone(self);
                thread::spawn(move || (registry.handlers[index])(input))
            })
            .collect();
        threads.into_iter().map(|t| t.join().expect("обработчик упал")).collect()
    }
}

fn main() {
    let mut r = Registry::new();
    r.add(|x| x + 1);
    println!("{:?}", Arc::new(r).run_all(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn results_keep_registration_order() {
        let mut r = Registry::new();
        r.add(|x| x + 1);
        r.add(|x| x * 2);
        r.add(|x| x * x);
        assert_eq!(Arc::new(r).run_all(5), vec![6, 10, 25]);
    }

    #[test]
    fn handlers_can_share_state() {
        let total = Arc::new(AtomicU32::new(0));
        let mut r = Registry::new();
        for _ in 0..4 {
            let total = Arc::clone(&total);
            r.add(move |x| {
                total.fetch_add(x, Ordering::SeqCst);
                x
            });
        }
        Arc::new(r).run_all(10);
        assert_eq!(total.load(Ordering::SeqCst), 40);
    }

    #[test]
    fn handlers_really_run_in_parallel() {
        let barrier = Arc::new(Barrier::new(3));
        let mut r = Registry::new();
        for i in 0..3 {
            let barrier = Arc::clone(&barrier);
            r.add(move |x| {
                barrier.wait();
                x + i
            });
        }
        let registry = Arc::new(r);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(registry.run_all(100));
        });
        let got = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("три обработчика ждут друг друга у барьера: если они запускаются по очереди, барьер не откроется никогда");
        assert_eq!(got, vec![100, 101, 102]);
    }

    #[test]
    fn registry_is_shareable_between_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Registry>();
    }
}
