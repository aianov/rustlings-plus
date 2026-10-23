use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

fn main() {}

pub struct Followers {
    processed: Mutex<HashSet<String>>,
    counts: Mutex<HashMap<String, u32>>,
    db_latency: Duration,
}

impl Followers {
    pub fn new(db_latency: Duration) -> Followers {
        Followers { processed: Mutex::new(HashSet::new()), counts: Mutex::new(HashMap::new()), db_latency }
    }

    pub fn followers(&self, user: &str) -> u32 {
        self.counts.lock().unwrap().get(user).copied().unwrap_or(0)
    }

    pub fn on_followed(&self, event_id: &str, followee: &str) -> bool {
        if !self.processed.lock().unwrap().insert(event_id.to_string()) {
            return false;
        }
        std::thread::sleep(self.db_latency);
        *self.counts.lock().unwrap().entry(followee.to_string()).or_default() += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn a_sequential_repeat_is_ignored() {
        let followers = Followers::new(Duration::ZERO);
        assert!(followers.on_followed("e1", "alice"), "первый раз");
        assert!(!followers.on_followed("e1", "alice"), "второй раз");
        assert_eq!(followers.followers("alice"), 1, "один подписчик");
    }

    #[test]
    fn simultaneous_deliveries_of_one_event_count_once() {
        for round in 0..5 {
            let followers = Arc::new(Followers::new(Duration::from_millis(30)));
            let barrier = Arc::new(Barrier::new(8));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let (followers, barrier) = (followers.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        followers.on_followed("e1", "alice")
                    })
                })
                .collect();
            let applied = handles.into_iter().map(|h| h.join().unwrap()).filter(|applied| *applied).count();
            assert_eq!(applied, 1, "раунд {round}: событие применили {applied} раз, а должно быть один: проверка и отметка не одним шагом");
            assert_eq!(followers.followers("alice"), 1, "раунд {round}: у Алисы один новый подписчик");
        }
    }

    #[test]
    fn different_events_at_the_same_time_are_all_counted() {
        let followers = Arc::new(Followers::new(Duration::from_millis(10)));
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (followers, barrier) = (followers.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    followers.on_followed(&format!("e{i}"), "alice")
                })
            })
            .collect();
        for handle in handles {
            assert!(handle.join().unwrap(), "разные события не мешают друг другу");
        }
        assert_eq!(followers.followers("alice"), 8, "восемь разных подписок, восемь подписчиков");
    }
}
