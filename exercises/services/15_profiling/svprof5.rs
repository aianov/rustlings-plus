// Все сессии шлюза лежат в одном `Mutex<HashMap>`. На тестовом стенде это незаметно, а под нагрузкой десять тысяч пользователей
// стоят в очереди к одному замку, даже когда обновляют разные записи: пока один пользователь «трогает» свою сессию (под замком
// бывает и обращение к стороннему сервису), остальные ждут. Цель: сессии разложены по нескольким независимым замкам (шардам) так,
// что обращения к пользователям из разных шардов не мешают друг другу, а к одному шарду по-прежнему взаимоисключающие. Пользователи
// распределяются по шардам ровно, даже когда их номера идут с закономерностью (кратные шестнадцати, например).


use std::collections::HashMap;
use std::sync::Mutex;

fn main() {}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Session {
    pub hits: u64,
    pub last_seen: u64,
}

pub struct Sessions {
    all: Mutex<HashMap<u64, Session>>,
}

impl Sessions {
    pub fn new(shards: usize) -> Sessions {
        let _ = shards;
        Sessions { all: Mutex::new(HashMap::new()) }
    }

    pub fn shard_of(&self, user: u64) -> usize {
        let _ = user;
        0
    }

    pub fn with<R>(&self, user: u64, f: impl FnOnce(&mut Session) -> R) -> R {
        let mut all = self.all.lock().expect("сессии не отравлены");
        f(all.entry(user).or_default())
    }

    pub fn remove(&self, user: u64) -> Option<Session> {
        self.all.lock().expect("сессии не отравлены").remove(&user)
    }

    pub fn len(&self) -> usize {
        self.all.lock().expect("сессии не отравлены").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn total_hits(&self) -> u64 {
        self.all.lock().expect("сессии не отравлены").values().map(|s| s.hits).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, Instant};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn keys_in_different_shards(s: &Sessions) -> (u64, u64) {
        let first = 1;
        let second = (2..10_000).find(|k| s.shard_of(*k) != s.shard_of(first)).expect("при нескольких шардах нашёлся пользователь из другого");
        (first, second)
    }

    fn keys_in_one_shard(s: &Sessions) -> (u64, u64) {
        let first = 1;
        let second = (2..10_000).find(|k| s.shard_of(*k) == s.shard_of(first)).expect("нашёлся пользователь из того же шарда");
        (first, second)
    }

    #[test]
    fn sessions_are_created_updated_and_removed() {
        let s = Sessions::new(8);
        assert!(s.is_empty());
        s.with(7, |x| x.hits += 3);
        s.with(7, |x| x.hits += 2);
        s.with(9, |x| x.last_seen = 55);
        assert_eq!(s.len(), 2);
        assert_eq!(s.total_hits(), 5);
        assert_eq!(s.with(7, |x| x.clone()), Session { hits: 5, last_seen: 0 });
        assert_eq!(s.remove(7), Some(Session { hits: 5, last_seen: 0 }));
        assert_eq!(s.remove(7), None);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn users_from_different_shards_do_not_wait_for_each_other() {
        let s = Arc::new(Sessions::new(8));
        let (a, b) = keys_in_different_shards(&s);
        let (tx, rx) = mpsc::channel();
        let holder = {
            let s = s.clone();
            std::thread::spawn(move || {
                s.with(a, |x| {
                    tx.send(()).expect("главный поток ждёт");
                    std::thread::sleep(MS(200));
                    x.hits += 1;
                })
            })
        };
        rx.recv().expect("первый пользователь взял свою запись");
        let started = Instant::now();
        s.with(b, |x| x.hits += 1);
        let took = started.elapsed();
        holder.join().expect("поток не упал");
        assert!(took < MS(120), "другой пользователь ждал {took:?}, пока первый держал свою запись 200 мс: один общий замок на все сессии");
    }

    #[test]
    fn users_from_the_same_shard_still_exclude_each_other() {
        let s = Arc::new(Sessions::new(4));
        let (a, b) = keys_in_one_shard(&s);
        let (tx, rx) = mpsc::channel();
        let holder = {
            let s = s.clone();
            std::thread::spawn(move || {
                s.with(a, |_| {
                    tx.send(()).expect("главный поток ждёт");
                    std::thread::sleep(MS(150));
                })
            })
        };
        rx.recv().expect("замок шарда взят");
        let started = Instant::now();
        s.with(b, |_| ());
        assert!(started.elapsed() >= MS(100), "в одном шарде работа должна идти по очереди: замок обязателен");
        holder.join().expect("поток не упал");
    }

    #[test]
    fn patterned_ids_still_spread_evenly() {
        let s = Sessions::new(16);
        let mut per_shard = [0usize; 16];
        for i in 0..8000u64 {
            per_shard[s.shard_of(i * 16)] += 1;
        }
        let (min, max) = (*per_shard.iter().min().expect("шарды есть"), *per_shard.iter().max().expect("шарды есть"));
        assert!(min >= 250 && max <= 800, "номера кратны шестнадцати, а шардов шестнадцать: остаток от деления складывает всех в один шард, нужна хеш-функция. По шардам {per_shard:?}");
    }

    #[test]
    fn the_same_user_always_lands_in_the_same_shard() {
        let s = Sessions::new(8);
        for user in 0..200u64 {
            assert_eq!(s.shard_of(user), s.shard_of(user));
            assert!(s.shard_of(user) < 8);
        }
    }

    #[test]
    fn parallel_updates_lose_nothing() {
        let s = Arc::new(Sessions::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let s = s.clone();
                std::thread::spawn(move || {
                    for i in 0..2000u64 {
                        s.with(i % 100, |x| x.hits += 1);
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("поток не упал");
        }
        assert_eq!(s.len(), 100);
        assert_eq!(s.total_hits(), 16_000, "ни одно обновление не потеряно");
    }
}
