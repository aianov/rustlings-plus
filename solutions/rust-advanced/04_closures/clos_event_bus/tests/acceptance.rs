use event_bus::{Event, EventBus};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<String>>>;

fn recorder(log: &Log, tag: &'static str) -> impl FnMut(&Event) + Send + 'static {
    let log = Arc::clone(log);
    move |e| log.lock().expect("журнал отравлен").push(format!("{tag}:{}:{}", e.topic, e.data))
}

fn lines(log: &Log) -> Vec<String> {
    log.lock().expect("журнал отравлен").clone()
}

#[test]
fn handlers_receive_events_of_their_topic_only() {
    let log: Log = Arc::default();
    let mut bus = EventBus::new();
    bus.subscribe("post", recorder(&log, "a"));
    bus.subscribe("like", recorder(&log, "b"));
    assert_eq!(bus.publish("post", "p1"), 1, "publish возвращает число вызванных обработчиков");
    assert_eq!(bus.publish("follow", "x"), 0, "на тему без подписчиков никто не откликается");
    assert_eq!(lines(&log), vec!["a:post:p1"]);
}

#[test]
fn call_order_is_subscription_order() {
    let log: Log = Arc::default();
    let mut bus = EventBus::new();
    bus.subscribe("t", recorder(&log, "first"));
    bus.subscribe("t", recorder(&log, "second"));
    bus.subscribe("t", recorder(&log, "third"));
    bus.publish("t", "1");
    assert_eq!(lines(&log), vec!["first:t:1", "second:t:1", "third:t:1"], "обработчики вызываются в порядке подписки");
}

#[test]
fn wildcard_gets_everything_in_global_order() {
    let log: Log = Arc::default();
    let mut bus = EventBus::new();
    bus.subscribe("a", recorder(&log, "x"));
    bus.subscribe("*", recorder(&log, "all"));
    bus.subscribe("a", recorder(&log, "y"));
    assert_eq!(bus.publish("a", "1"), 3);
    assert_eq!(bus.publish("b", "2"), 1, "подписка * получает и чужие темы");
    assert_eq!(lines(&log), vec!["x:a:1", "all:a:1", "y:a:1", "all:b:2"], "порядок общий для обычных и * подписок");
}

#[test]
fn unsubscribe_by_id() {
    let log: Log = Arc::default();
    let mut bus = EventBus::new();
    let first = bus.subscribe("t", recorder(&log, "one"));
    let second = bus.subscribe("t", recorder(&log, "two"));
    assert_ne!(first, second, "id подписок различаются");
    assert!(bus.unsubscribe(first));
    assert!(!bus.unsubscribe(first), "повторная отписка возвращает false");
    assert!(!bus.unsubscribe(9999), "неизвестный id тоже false");
    bus.publish("t", "x");
    assert_eq!(lines(&log), vec!["two:t:x"], "отписанный обработчик больше не вызывается");
}

#[test]
fn ids_are_never_reused() {
    let mut bus = EventBus::new();
    let a = bus.subscribe("t", |_| {});
    bus.unsubscribe(a);
    let b = bus.subscribe("t", |_| {});
    assert_ne!(a, b, "id освободившейся подписки не должен выдаваться повторно");
}

#[test]
fn handlers_keep_mutable_state() {
    let mut bus = EventBus::new();
    let seen = Arc::new(AtomicUsize::new(0));
    let seen2 = Arc::clone(&seen);
    let mut local_count = 0;
    bus.subscribe("t", move |_| {
        local_count += 1;
        seen2.store(local_count, Ordering::SeqCst);
    });
    for _ in 0..4 {
        bus.publish("t", "");
    }
    assert_eq!(seen.load(Ordering::SeqCst), 4, "FnMut-обработчик накапливает состояние между вызовами");
}

#[test]
fn counting_subscribers() {
    let mut bus = EventBus::new();
    assert_eq!(bus.subscribers("t"), 0);
    let id = bus.subscribe("t", |_| {});
    bus.subscribe("*", |_| {});
    bus.subscribe("other", |_| {});
    assert_eq!(bus.subscribers("t"), 2, "считаются и * подписки");
    bus.unsubscribe(id);
    assert_eq!(bus.subscribers("t"), 1);
}

#[test]
fn the_bus_can_be_shared_between_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<EventBus>();
    let hits = Arc::new(AtomicUsize::new(0));
    let mut bus = EventBus::new();
    for _ in 0..2 {
        let hits = Arc::clone(&hits);
        bus.subscribe("t", move |_| {
            hits.fetch_add(1, Ordering::SeqCst);
        });
    }
    let bus = Arc::new(Mutex::new(bus));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let bus = Arc::clone(&bus);
            std::thread::spawn(move || {
                for i in 0..50 {
                    bus.lock().expect("шина отравлена").publish("t", &i.to_string());
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("поток упал");
    }
    assert_eq!(hits.load(Ordering::SeqCst), 400, "4 потока по 50 событий на 2 подписчика");
}
