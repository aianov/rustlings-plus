use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Default)]
pub struct ProfileCache {
    map: RwLock<HashMap<u32, Arc<String>>>,
}

impl ProfileCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.map.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_or_load(&self, id: u32, load: impl FnOnce(u32) -> String) -> Arc<String> {
        if let Some(found) = self.map.read().unwrap().get(&id) {
            return Arc::clone(found);
        }
        let mut map = self.map.write().unwrap();
        if let Some(found) = map.get(&id) {
            return Arc::clone(found);
        }
        let value = Arc::new(load(id));
        map.insert(id, Arc::clone(&value));
        value
    }
}

fn main() {
    let cache = ProfileCache::new();
    println!("{}", cache.get_or_load(1, |id| format!("профиль {id}")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;
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
    fn a_miss_loads_and_a_hit_does_not() {
        let (first, second, loads) = within(2, "кэш", || {
            let cache = ProfileCache::new();
            let loads = AtomicUsize::new(0);
            let first = cache.get_or_load(7, |id| {
                loads.fetch_add(1, Ordering::SeqCst);
                format!("профиль {id}")
            });
            let second = cache.get_or_load(7, |_| {
                loads.fetch_add(1, Ordering::SeqCst);
                "другой".to_string()
            });
            assert_eq!(cache.len(), 1);
            (first, second, loads.load(Ordering::SeqCst))
        });
        assert_eq!(first.as_str(), "профиль 7");
        assert!(Arc::ptr_eq(&first, &second), "второй вызов должен отдать тот же Arc из кэша");
        assert_eq!(loads, 1);
    }

    #[test]
    fn different_ids_are_cached_separately() {
        within(2, "разные id", || {
            let cache = ProfileCache::new();
            assert_eq!(cache.get_or_load(1, |_| "a".into()).as_str(), "a");
            assert_eq!(cache.get_or_load(2, |_| "b".into()).as_str(), "b");
            assert_eq!(cache.get_or_load(1, |_| "новый".into()).as_str(), "a");
            assert_eq!(cache.len(), 2);
        });
    }

    #[test]
    fn racing_threads_load_a_profile_once() {
        let (loads, same) = within(4, "гонка за профиль", || {
            let cache = Arc::new(ProfileCache::new());
            let loads = Arc::new(AtomicUsize::new(0));
            let barrier = Arc::new(Barrier::new(8));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let cache = Arc::clone(&cache);
                    let loads = Arc::clone(&loads);
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        cache.get_or_load(42, |id| {
                            loads.fetch_add(1, Ordering::SeqCst);
                            thread::sleep(Duration::from_millis(30));
                            format!("профиль {id}")
                        })
                    })
                })
                .collect();
            let values: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            let same = values.iter().all(|v| Arc::ptr_eq(v, &values[0]));
            (loads.load(Ordering::SeqCst), same)
        });
        assert_eq!(loads, 1, "загрузчик вызван несколько раз: после взятия записи нужно проверить кэш ещё раз");
        assert!(same, "все потоки должны получить один и тот же Arc");
    }
}
