use feed::FeedService;
use feed::diag::watch;
use feed::render::render_page;
use feed::request::parse_request;
use feed::storage::{LATENCY, Post};
use feed::store::SessionStore;
use futures::future::join_all;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[allow(unsafe_code)]
mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<u64> = const { Cell::new(0) };
    }

    pub struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT.with(|c| c.set(c.get() + 1));
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            COUNT.with(|c| c.set(c.get() + 1));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    pub fn allocations<R>(f: impl FnOnce() -> R) -> (R, u64) {
        let before = COUNT.with(Cell::get);
        let result = f();
        (result, COUNT.with(Cell::get) - before)
    }
}

const MS: fn(u64) -> Duration = Duration::from_millis;
const BODY: &str = r#"{"user":7,"limit":3,"tag":"news"}"#;

async fn one(svc: &FeedService, body: &str) -> Result<String, String> {
    let mut out = String::new();
    svc.handle(body, &mut out).await.map(|()| out)
}

#[tokio::test]
async fn the_page_is_what_it_was() {
    let out = one(&FeedService::new(), BODY).await.expect("запрос обработан");
    let got: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("ответ не JSON ({e}): {out}"));
    assert_eq!(
        got,
        json!({"user": 7, "posts": [
            {"id": 1, "author": "author7", "text": "пост 1 пользователя 7", "likes": 3},
            {"id": 2, "author": "author7", "text": "цитата \"раз\"\nи строка с \\ чертой", "likes": 6},
            {"id": 3, "author": "author7", "text": "пост 3 пользователя 7", "likes": 9},
        ]})
    );
}

#[tokio::test]
async fn limit_and_bad_input() {
    let svc = FeedService::new();
    let all = one(&svc, r#"{"user":1,"limit":50}"#).await.expect("запрос обработан");
    assert_eq!(serde_json::from_str::<Value>(&all).expect("JSON")["posts"].as_array().map(Vec::len), Some(5), "больше пяти постов в хранилище нет");
    let none = one(&svc, r#"{"user":1,"limit":0}"#).await.expect("запрос обработан");
    assert_eq!(serde_json::from_str::<Value>(&none).expect("JSON")["posts"], json!([]));
    assert!(one(&svc, "не json").await.is_err());
    assert!(one(&svc, r#"{"user":1}"#).await.is_err(), "не хватает limit");
}

#[tokio::test]
async fn a_request_leaves_a_trace_in_the_session() {
    let svc = FeedService::new();
    for _ in 0..3 {
        one(&svc, BODY).await.expect("запрос обработан");
    }
    assert_eq!(svc.sessions().total_hits(), 3);
}

#[tokio::test]
async fn nothing_holds_the_runtime_thread() {
    let svc = FeedService::new();
    let watched: Vec<_> = (0..6)
        .map(|i| {
            let svc = &svc;
            watch(async move { one(svc, &format!(r#"{{"user":{i},"limit":2}}"#)).await }, MS(20))
        })
        .collect();
    let (futures, stats): (Vec<_>, Vec<_>) = watched.into_iter().unzip();
    for result in join_all(futures).await {
        result.expect("запрос обработан");
    }
    let slow: u64 = stats.iter().map(|s| s.slow_polls()).sum();
    let longest = stats.iter().map(|s| s.longest()).max().unwrap_or_default();
    assert_eq!(slow, 0, "самый долгий опрос длился {longest:?}: в async-коде есть синхронный вызов, который держит поток");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eight_requests_at_once_cost_about_one_storage_latency() {
    let svc = Arc::new(FeedService::new());
    let started = Instant::now();
    let tasks: Vec<_> = (0..8)
        .map(|i| {
            let svc = svc.clone();
            tokio::spawn(async move { one(&svc, &format!(r#"{{"user":{i},"limit":2}}"#)).await })
        })
        .collect();
    for t in tasks {
        t.await.expect("задача не упала").expect("запрос обработан");
    }
    let took = started.elapsed();
    assert!(took < LATENCY * 3, "восемь запросов заняли {took:?} при задержке хранилища {LATENCY:?}: хранилище читается по очереди, а не параллельно");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sessions_of_different_users_are_touched_in_parallel() {
    let now = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (n, p) = (now.clone(), peak.clone());
    let svc = Arc::new(FeedService::with_hook(Arc::new(move |_, _| {
        let inside = n.fetch_add(1, Ordering::SeqCst) + 1;
        p.fetch_max(inside, Ordering::SeqCst);
        std::thread::sleep(MS(100));
        n.fetch_sub(1, Ordering::SeqCst);
    })));
    let mut users: Vec<u64> = Vec::new();
    for candidate in 0..10_000 {
        if users.iter().all(|u| svc.sessions().shard_of(*u) != svc.sessions().shard_of(candidate)) {
            users.push(candidate);
        }
        if users.len() == 4 {
            break;
        }
    }
    assert_eq!(users.len(), 4, "у пользователей должны быть разные шарды");
    let tasks: Vec<_> = users
        .into_iter()
        .map(|u| {
            let svc = svc.clone();
            tokio::spawn(async move { one(&svc, &format!(r#"{{"user":{u},"limit":1}}"#)).await })
        })
        .collect();
    for t in tasks {
        t.await.expect("задача не упала").expect("запрос обработан");
    }
    assert!(peak.load(Ordering::SeqCst) >= 2, "работа с сессиями разных пользователей шла строго по очереди: один общий замок на все сессии");
}

#[test]
fn the_store_shards_are_independent_and_exclusive_inside() {
    let store = Arc::new(SessionStore::new(8));
    let other = (2..10_000).find(|k| store.shard_of(*k) != store.shard_of(1)).expect("есть пользователь из другого шарда");
    let same = (2..10_000).find(|k| store.shard_of(*k) == store.shard_of(1)).expect("есть пользователь из того же шарда");
    for (second, must_wait) in [(other, false), (same, true)] {
        let (tx, rx) = mpsc::channel();
        let holder = {
            let store = store.clone();
            std::thread::spawn(move || {
                store.with(1, |_| {
                    tx.send(()).expect("главный поток ждёт");
                    std::thread::sleep(MS(200));
                })
            })
        };
        rx.recv().expect("замок взят");
        let started = Instant::now();
        store.with(second, |s| s.hits += 1);
        let took = started.elapsed();
        holder.join().expect("поток не упал");
        if must_wait {
            assert!(took >= MS(120), "пользователи одного шарда обязаны ждать друг друга: {took:?}");
        } else {
            assert!(took < MS(120), "пользователь другого шарда ждал {took:?}");
        }
    }
}

#[test]
fn the_hot_path_barely_touches_the_allocator() {
    let posts: Vec<Post> = (1..=5).map(|id| Post { id, author: "author7".to_string(), text: "цитата \"раз\"\nтекст".to_string(), likes: 3 }).collect();
    let body = r#"{"user":7,"limit":5,"tag":"news"}"#;
    let mut out = String::with_capacity(1024);
    parse_request(body).expect("разогрев");
    render_page(7, &posts, 5, &mut out);
    let ((), allocs) = counting::allocations(|| {
        for _ in 0..200 {
            let request = parse_request(body).expect("разобрали");
            render_page(request.user, &posts, request.limit, &mut out);
        }
    });
    assert!(allocs <= 20, "на 200 запросов {allocs} выделений: на запрос их должно быть не больше одного, а лучше ни одного");
    assert!(out.starts_with("{\"user\":7,\"posts\":[{\"id\":1"), "{out}");
}

#[test]
fn an_escaped_tag_is_still_parsed() {
    let request = parse_request(r#"{"user":1,"limit":1,"tag":"а\nб"}"#).expect("разобрали");
    assert_eq!(request.tag, "а\nб");
    assert_eq!(parse_request(r#"{"user":1,"limit":1}"#).expect("разобрали").tag, "", "без метки это пустая строка");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn under_load_p99_stays_near_the_storage_latency() {
    let svc = Arc::new(FeedService::with_hook(Arc::new(|_, _| std::thread::sleep(MS(3)))));
    let mut latencies: Vec<Duration> = Vec::new();
    for round in 0..4 {
        let tasks: Vec<_> = (0..16)
            .map(|i| {
                let svc = svc.clone();
                let started = Instant::now();
                tokio::spawn(async move {
                    let user = round * 100 + i;
                    let out = one(&svc, &format!(r#"{{"user":{user},"limit":3,"tag":"x"}}"#)).await;
                    (out.is_ok(), started.elapsed())
                })
            })
            .collect();
        for t in tasks {
            let (ok, took) = t.await.expect("задача не упала");
            assert!(ok, "запрос под нагрузкой должен проходить");
            latencies.push(took);
        }
    }
    latencies.sort();
    let p99 = latencies[(latencies.len() * 99).div_ceil(100) - 1];
    assert!(p99 < MS(160), "p99 под нагрузкой {p99:?} при задержке хранилища {LATENCY:?} и пустой работе с сессиями: узкие места остались");
}
