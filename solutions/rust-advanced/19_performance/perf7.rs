use std::hash::Hash;

use std::collections::{HashSet, VecDeque};

pub struct Queue<T> {
    items: VecDeque<T>,
}

impl<T> Queue<T> {
    pub fn new() -> Queue<T> {
        Queue { items: VecDeque::new() }
    }

    pub fn push(&mut self, value: T) {
        self.items.push_back(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

pub fn unique_in_order<T: Eq + Hash + Clone>(items: &[T]) -> Vec<T> {
    let mut seen: HashSet<&T> = HashSet::with_capacity(items.len());
    items.iter().filter(|item| seen.insert(item)).cloned().collect()
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Queue::new()
    }
}

fn main() {
    let mut queue = Queue::new();
    queue.push(1);
    println!("{:?} {:?}", queue.pop(), unique_in_order(&[3, 1, 3, 2, 1]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
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
        fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }
    const SRC: &str = include_str!("perf7.rs");

    thread_local! {
        static EQ_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    #[derive(Clone, Debug)]
    struct Tracked(u32);

    impl PartialEq for Tracked {
        fn eq(&self, other: &Self) -> bool {
            EQ_CALLS.set(EQ_CALLS.get() + 1);
            self.0 == other.0
        }
    }

    impl Eq for Tracked {}

    impl Hash for Tracked {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    #[test]
    fn queue_is_first_in_first_out() {
        let mut q = Queue::new();
        assert_eq!(q.pop(), None::<u8>);
        q.push(1);
        q.push(2);
        assert_eq!(q.pop(), Some(1));
        q.push(3);
        assert_eq!((q.len(), q.pop(), q.pop(), q.pop()), (2, Some(2), Some(3), None));
        assert!(q.is_empty());
    }

    #[test]
    fn queue_does_not_shift_the_tail() {
        let code = code_only();
        assert!(!code.contains(".remove(0)"), "remove(0) сдвигает весь вектор на каждое извлечение: нужна структура, у которой начало дёшево");
        within(15, "очередь на 100 000 заданий", || {
            let mut q = Queue::new();
            for i in 0..100_000u64 {
                q.push([i; 8]);
            }
            let mut sum = 0;
            while let Some(job) = q.pop() {
                sum += job[0];
            }
            assert_eq!(sum, 100_000 * 99_999 / 2);
        });
    }

    #[test]
    fn unique_keeps_first_occurrences_in_order() {
        assert_eq!(unique_in_order(&[3, 1, 3, 2, 1, 4]), vec![3, 1, 2, 4]);
        assert_eq!(unique_in_order::<u8>(&[]), Vec::<u8>::new());
        assert_eq!(unique_in_order(&["b".to_string(), "a".to_string(), "b".to_string()]), vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn unique_does_not_compare_everything_with_everything() {
        let items: Vec<Tracked> = (0..6_000).map(|n| Tracked(n % 3_000)).collect();
        EQ_CALLS.set(0);
        let unique = unique_in_order(&items);
        let calls = EQ_CALLS.get();
        assert_eq!(unique.len(), 3_000);
        assert!(unique.iter().enumerate().all(|(i, t)| t.0 as usize == i), "порядок первого появления");
        assert!(calls < 60_000, "{calls} сравнений на 6000 элементов: поиск по списку квадратичен, нужна структура с быстрой проверкой «уже было»");
    }
}
