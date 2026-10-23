use fanout::{Fanout, FollowError, ProfileStore, PublishError, RegisterError, Sink, Stats, UserId};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const SRC: &str = include_str!("../src/lib.rs");

fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
    }
}

#[derive(Default)]
struct Store {
    loads: AtomicUsize,
    inside: AtomicUsize,
    peak: AtomicUsize,
    delay_ms: u64,
}

impl Store {
    fn slow(delay_ms: u64) -> Arc<Store> {
        Arc::new(Store { delay_ms, ..Store::default() })
    }
}

impl ProfileStore for Store {
    fn load(&self, id: UserId) -> Option<String> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(self.delay_ms));
        self.inside.fetch_sub(1, Ordering::SeqCst);
        (id <= 100).then(|| format!("Имя {id}"))
    }
}

#[derive(Default)]
struct Recorder {
    log: Mutex<Vec<(UserId, String)>>,
    inside: AtomicUsize,
    peak: AtomicUsize,
    delay_ms: u64,
    fail_for: Option<UserId>,
}

impl Sink for Recorder {
    fn deliver(&self, to: UserId, text: &str) -> Result<(), String> {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(self.delay_ms));
        self.inside.fetch_sub(1, Ordering::SeqCst);
        if self.fail_for == Some(to) {
            return Err("получатель недоступен".to_string());
        }
        self.log.lock().unwrap().push((to, text.to_string()));
        Ok(())
    }
}

fn service(workers: usize, sink: Arc<Recorder>) -> Fanout {
    Fanout::new(workers, Store::slow(0), sink)
}

fn star(fanout: &Fanout, friends: u32) -> UserId {
    let author = fanout.register("author").unwrap();
    for i in 0..friends {
        let friend = fanout.register(&format!("friend{i}")).unwrap();
        fanout.befriend(author, friend).unwrap();
    }
    author
}

#[test]
fn registration_hands_out_sequential_ids_and_checks_logins() {
    within(5, "регистрация", || {
        let fanout = service(1, Arc::default());
        assert_eq!(fanout.register("ann"), Ok(1));
        assert_eq!(fanout.register("bob_2"), Ok(2));
        assert_eq!(fanout.register("ann"), Err(RegisterError::Taken));
        assert_eq!(fanout.register(""), Err(RegisterError::Invalid));
        assert_eq!(fanout.register("с пробелом"), Err(RegisterError::Invalid));
        assert_eq!(fanout.register("кириллица"), Err(RegisterError::Invalid));
        assert_eq!(fanout.register(&"x".repeat(21)), Err(RegisterError::Invalid));
        assert_eq!(fanout.register(&"x".repeat(20)), Ok(3), "ошибки не должны сжигать номера");
    });
}

#[test]
fn racing_for_one_login_has_a_single_winner_and_no_burnt_ids() {
    let (wins, next) = within(10, "гонка за логин", || {
        let fanout = Arc::new(service(1, Arc::default()));
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let fanout = Arc::clone(&fanout);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    fanout.register("ann")
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(results.iter().filter(|r| r.is_err()).all(|r| *r == Err(RegisterError::Taken)));
        (results.iter().filter(|r| r.is_ok()).count(), fanout.register("bob"))
    });
    assert_eq!(wins, 1, "логин достался нескольким пользователям");
    assert_eq!(next, Ok(2), "проигравшие гонку не должны занимать номера");
}

#[test]
fn profiles_are_loaded_once_and_shared() {
    within(5, "кэш профилей", || {
        let store = Store::slow(0);
        let fanout = Fanout::new(1, store.clone(), Arc::new(Recorder::default()));
        let first = fanout.profile(5).unwrap();
        assert_eq!(first.as_str(), "Имя 5");
        for _ in 0..10 {
            assert!(Arc::ptr_eq(&first, &fanout.profile(5).unwrap()), "кэш должен отдавать тот же Arc");
        }
        assert_eq!(store.loads.load(Ordering::SeqCst), 1, "повторные запросы не должны ходить в хранилище");
        assert_eq!(fanout.profile(500), None, "неизвестного профиля нет");
    });
}

#[test]
fn threads_asking_for_the_same_profile_get_one_arc() {
    let same = within(10, "один профиль из многих потоков", || {
        let fanout = Arc::new(Fanout::new(1, Store::slow(30), Arc::new(Recorder::default())));
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let fanout = Arc::clone(&fanout);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    fanout.profile(7).unwrap()
                })
            })
            .collect();
        let values: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        values.iter().all(|v| Arc::ptr_eq(v, &values[0]))
    });
    assert!(same, "все потоки должны получить один и тот же Arc");
}

#[test]
fn slow_loads_of_different_profiles_overlap() {
    let peak = within(10, "загрузка разных профилей", || {
        let store = Store::slow(80);
        let fanout = Arc::new(Fanout::new(1, store.clone(), Arc::new(Recorder::default())));
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (1..=4)
            .map(|id| {
                let fanout = Arc::clone(&fanout);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    fanout.profile(id).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        store.peak.load(Ordering::SeqCst)
    });
    assert!(peak >= 2, "хранилище вызывают под замком кэша: профили разных людей грузятся по одному");
}

#[test]
fn friendship_is_mutual_idempotent_and_validated() {
    within(5, "дружба", || {
        let fanout = service(1, Arc::default());
        let (a, b, c) = (fanout.register("a").unwrap(), fanout.register("b").unwrap(), fanout.register("c").unwrap());
        assert_eq!(fanout.befriend(c, a), Ok(()));
        assert_eq!(fanout.befriend(a, b), Ok(()));
        assert_eq!(fanout.befriend(b, a), Ok(()));
        assert_eq!(fanout.friends(a), vec![b, c], "друзья по возрастанию, без повторов");
        assert_eq!(fanout.friends(b), vec![a]);
        assert_eq!(fanout.friends(c), vec![a]);
        assert_eq!(fanout.befriend(a, a), Err(FollowError::SameUser));
        assert_eq!(fanout.befriend(a, 99), Err(FollowError::UnknownUser(99)));
        assert_eq!(fanout.befriend(98, a), Err(FollowError::UnknownUser(98)));
        assert!(fanout.friends(99).is_empty());
    });
}

#[test]
fn opposite_befriending_under_load_does_not_deadlock() {
    let consistent = within(8, "встречная дружба", || {
        let fanout = Arc::new(service(1, Arc::default()));
        for i in 0..4 {
            fanout.register(&format!("u{i}")).unwrap();
        }
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4_u32)
            .map(|t| {
                let fanout = Arc::clone(&fanout);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    for round in 0..4000_u32 {
                        let a = 1 + (t + round) % 4;
                        let b = 1 + (t + round + 1 + round % 3) % 4;
                        if a != b {
                            fanout.befriend(a, b).unwrap();
                            fanout.befriend(b, a).unwrap();
                        }
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        (1..=4).all(|a| fanout.friends(a).iter().all(|&b| fanout.friends(b).contains(&a)))
    });
    assert!(consistent, "дружба должна быть взаимной у всех пар");
}

#[test]
fn publishing_notifies_every_friend_with_the_right_text() {
    within(10, "публикация", || {
        let sink = Arc::new(Recorder::default());
        let fanout = service(2, sink.clone());
        let author = star(&fanout, 3);
        assert_eq!(fanout.register("stranger"), Ok(5));
        assert_eq!(fanout.publish(author, "привет"), Ok(3));
        let stats = fanout.shutdown();
        assert_eq!(stats, Stats { posts: 1, queued: 3, delivered: 3, failed: 0 });
        let mut log = sink.log.lock().unwrap().clone();
        log.sort();
        let expected: Vec<(UserId, String)> = (2..=4).map(|id| (id, "author опубликовал: привет".to_string())).collect();
        assert_eq!(log, expected, "уведомления должны получить ровно друзья автора");
    });
}

#[test]
fn publish_returns_before_the_deliveries_finish() {
    let (elapsed, stats) = within(10, "публикация в фоне", || {
        let sink = Arc::new(Recorder { delay_ms: 150, ..Recorder::default() });
        let fanout = service(1, sink);
        let author = star(&fanout, 3);
        let started = Instant::now();
        fanout.publish(author, "быстро").unwrap();
        let elapsed = started.elapsed();
        (elapsed, fanout.shutdown())
    });
    assert!(elapsed < Duration::from_millis(100), "publish ждёт отправки: уведомления должны уходить в фоне, а не в потоке автора");
    assert_eq!(stats.delivered, 3, "shutdown должен дождаться отправки");
}

#[test]
fn deliveries_run_on_several_workers() {
    let peak = within(10, "рабочие пула", || {
        let sink = Arc::new(Recorder { delay_ms: 100, ..Recorder::default() });
        let fanout = service(3, sink.clone());
        let author = star(&fanout, 6);
        fanout.publish(author, "пост").unwrap();
        fanout.shutdown();
        sink.peak.load(Ordering::SeqCst)
    });
    assert!(peak >= 2, "три рабочих должны отправлять одновременно");
}

#[test]
fn failures_are_counted_not_lost() {
    within(10, "ошибки отправки", || {
        let sink = Arc::new(Recorder { fail_for: Some(3), ..Recorder::default() });
        let fanout = service(2, sink);
        let author = star(&fanout, 3);
        fanout.publish(author, "эй").unwrap();
        assert_eq!(fanout.shutdown(), Stats { posts: 1, queued: 3, delivered: 2, failed: 1 });
    });
}

#[test]
fn shutdown_drains_the_queue_and_closes_publishing() {
    let (stats, after, again) = within(15, "мягкая остановка", || {
        let sink = Arc::new(Recorder { delay_ms: 5, ..Recorder::default() });
        let fanout = service(1, sink);
        let author = star(&fanout, 10);
        for i in 0..4 {
            fanout.publish(author, &format!("пост {i}")).unwrap();
        }
        let stats = fanout.shutdown();
        let after = (fanout.publish(author, "поздно"), fanout.stats());
        (stats, after, fanout.shutdown())
    });
    assert_eq!(stats, Stats { posts: 4, queued: 40, delivered: 40, failed: 0 }, "после shutdown все поставленные уведомления отправлены");
    assert_eq!(after.0, Err(PublishError::Closed));
    assert_eq!(after.1, stats, "закрытая публикация ничего не считает");
    assert_eq!(again, stats, "повторный shutdown безопасен");
}

#[test]
fn unknown_author_is_rejected() {
    within(5, "неизвестный автор", || {
        let fanout = service(1, Arc::default());
        assert_eq!(fanout.publish(7, "ничей пост"), Err(PublishError::UnknownUser));
        assert_eq!(fanout.stats(), Stats::default());
    });
}

#[test]
fn counters_are_exact_under_concurrent_publishing() {
    let stats = within(15, "параллельные публикации", || {
        let fanout = Arc::new(service(4, Arc::default()));
        let author = star(&fanout, 4);
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let fanout = Arc::clone(&fanout);
                thread::spawn(move || {
                    for i in 0..25 {
                        fanout.publish(author, &format!("пост {i}")).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        fanout.shutdown()
    });
    assert_eq!(stats, Stats { posts: 100, queued: 400, delivered: 400, failed: 0 }, "счётчики теряют обновления");
}

#[test]
fn the_design_uses_rwlock_atomics_and_no_polling() {
    let code: String = SRC
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(code.contains("RwLock"), "кэш профилей должен лежать на RwLock");
    assert!(code.contains("Atomic"), "счётчики должны быть атомарными");
    assert!(!code.contains("sleep"), "sleep в сервисе не нужен");
    let distinct: HashSet<&str> = ["Mutex<", "RwLock<"].into_iter().filter(|kind| code.contains(kind)).collect();
    assert_eq!(distinct.len(), 2, "нужны и Mutex (списки друзей), и RwLock (кэш)");
}
