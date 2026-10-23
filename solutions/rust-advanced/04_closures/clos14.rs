use std::sync::{Arc, Mutex};

pub type Handler = Box<dyn Fn() -> u32 + Send + Sync + 'static>;

pub fn counting_handler() -> (Handler, Arc<Mutex<u32>>) {
    let counter = Arc::new(Mutex::new(0));
    let inner = Arc::clone(&counter);
    let handler: Handler = Box::new(move || {
        let mut value = inner.lock().expect("счётчик отравлен");
        *value += 1;
        *value
    });
    (handler, counter)
}

fn main() {
    let (h, c) = counting_handler();
    println!("{} {:?}", h(), c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn counts_in_one_thread() {
        let (handler, counter) = counting_handler();
        assert_eq!(handler(), 1);
        assert_eq!(handler(), 2);
        assert_eq!(*counter.lock().expect("счётчик отравлен"), 2, "счётчик снаружи видит то же значение");
    }

    #[test]
    fn counts_across_threads() {
        let (handler, counter) = counting_handler();
        let handler = Arc::new(handler);
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let handler = Arc::clone(&handler);
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        handler();
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("поток упал");
        }
        assert_eq!(*counter.lock().expect("счётчик отравлен"), 800, "ни одно обращение не должно потеряться");
    }
}
