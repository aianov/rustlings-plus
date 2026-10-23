use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::{self, Context};
use futures::StreamExt;
use nats_engine::js::{self, StreamSpec};
use nats_engine::{Bus, BusConfig, BusError, Meta, Subject};
use notify_service::bootstrap::NotifyApp;
use notify_service::common::{Config, Deliver};
use notify_service::repository::Store;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const DEFAULT_URL: &str = "nats://127.0.0.1:54222";

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn unique() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("t_{}_{nanos:x}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

struct World {
    root: String,
    publisher: Bus,
    js: Context,
}

impl Drop for World {
    fn drop(&mut self) {
        let (url, stream) = (url(), format!("{}_EVENTS", self.root));
        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
            runtime.block_on(async {
                if let Ok(client) = async_nats::connect(url.as_str()).await {
                    jetstream::new(client.clone()).delete_stream(&stream).await.ok();
                    client.flush().await.ok();
                }
            });
        })
        .join()
        .ok();
    }
}

impl World {
    async fn new() -> World {
        let world = World {
            root: unique(),
            publisher: Bus::connect(BusConfig::new(url())).await.expect("шина издателя"),
            js: jetstream::new(async_nats::connect(url()).await.expect("подключение")),
        };
        let spec = StreamSpec {
            name: format!("{}_EVENTS", world.root),
            subjects: vec![format!("{}.post.>", world.root), format!("{}.user.>", world.root), format!("{}.dead.>", world.root)],
            duplicate_window: Duration::from_secs(120),
        };
        js::ensure_stream(&world.publisher, &spec).await.expect("stream");
        world
    }

    fn config(&self) -> Config {
        Config::new(url(), self.root.clone())
    }

    fn subject(&self, tail: &[&str]) -> Subject {
        let mut tokens = vec![self.root.as_str()];
        tokens.extend_from_slice(tail);
        Subject::new(&tokens).expect("тема")
    }

    async fn event(&self, event_id: &str, tail: &[&str], body: Value) -> js::Published {
        js::publish_event(&self.publisher, &self.subject(tail), event_id, &body, &Meta::new("req-1", "trace-1").unwrap()).await.expect("публикация события")
    }

    async fn like(&self, post: u64) -> js::Published {
        self.event(&format!("like-{post}"), &["post", "liked", "alice"], json!({"actor": "bob", "post_id": post})).await
    }

    async fn ask(&self, what: &str, user: &str) -> Result<u32, BusError> {
        self.publisher.request(&self.subject(&[what, user]), &json!({}), &Meta::fresh(), Duration::from_secs(2)).await
    }

    async fn dead_letters(&self) -> Vec<String> {
        let stream = self.js.get_stream(format!("{}_EVENTS", self.root)).await.unwrap();
        let consumer = stream
            .create_consumer(pull::Config { ack_policy: AckPolicy::None, filter_subject: format!("{}.dead.>", self.root), ..Default::default() })
            .await
            .unwrap();
        let mut batch = consumer.fetch().max_messages(50).expires(Duration::from_millis(300)).messages().await.unwrap();
        let mut out = Vec::new();
        while let Some(message) = batch.next().await {
            out.push(String::from_utf8_lossy(&message.unwrap().payload).into_owned());
        }
        out
    }
}

async fn eventually(what: &str, within: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !condition() {
        assert!(Instant::now() < deadline, "за {within:?} не наступило: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

const SECS: Duration = Duration::from_secs(6);

#[tokio::test]
async fn events_become_notifications_and_a_restart_loses_nothing_and_replays_nothing() {
    let world = World::new().await;
    let store = Store::new();
    let app = NotifyApp::start(world.config(), store.clone()).await.expect("запуск");
    for post in 1..=3 {
        world.like(post).await;
    }
    eventually("три уведомления", SECS, || app.unread("alice") == 3).await;
    app.stop().await;
    for post in 4..=7 {
        world.like(post).await;
    }
    assert_eq!(store.transaction(|tx| tx.unread_count("alice")), 3, "пока сервис выключен, новые события ждут в stream");
    let again = NotifyApp::start(world.config(), store.clone()).await.expect("перезапуск");
    eventually("четыре события, пришедшие за время простоя", SECS, || again.unread("alice") == 7).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    let stats = again.stats();
    assert_eq!((stats.received, stats.stored), (4, 4), "после перезапуска обработаны только новые события, ничего не потеряно и старое не прочитано заново: {stats:?}");
    assert_eq!(again.unread("alice"), 7, "итог: семь уведомлений");
    again.stop().await;
}

#[tokio::test]
async fn the_same_event_published_twice_is_one_notification() {
    let world = World::new().await;
    let app = NotifyApp::start(world.config(), Store::new()).await.expect("запуск");
    let first = world.like(9).await;
    let repeat = world.like(9).await;
    assert!(!first.duplicate && repeat.duplicate && first.sequence == repeat.sequence, "второй публикации с тем же именем брокер записывать не стал: {first:?} {repeat:?}");
    eventually("уведомление создано", SECS, || app.unread("alice") == 1).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(app.stats().received, 1, "сервис получил событие один раз");
    app.stop().await;
}

#[tokio::test]
async fn bad_events_go_to_the_dead_subject_and_do_not_block_the_good_ones() {
    let world = World::new().await;
    let app = NotifyApp::start(world.config(), Store::new()).await.expect("запуск");
    world.js.publish(format!("{}.post.liked.alice", world.root), "это не JSON".into()).await.unwrap().await.unwrap();
    world.event("bad-actor", &["post", "liked", "alice"], json!({"actor": "bad actor!", "post_id": 1})).await;
    world.event("no-post", &["post", "commented", "alice"], json!({"actor": "bob"})).await;
    world.like(5).await;
    eventually("годное событие обработано", SECS, || app.unread("alice") == 1).await;
    eventually("три плохих события в мёртвой теме", SECS, || app.stats().rejected == 2).await;
    let dead = world.dead_letters().await;
    assert_eq!(dead.len(), 3, "три плохих события лежат в мёртвой теме: {dead:?}");
    assert!(dead.iter().any(|d| d == "это не JSON"), "в том числе битый JSON");
    let stats = app.stats();
    assert_eq!((stats.stored, stats.rejected), (1, 2), "{stats:?}");
    app.stop().await;
}

#[tokio::test]
async fn two_instances_share_the_durable_and_answer_requests() {
    let world = World::new().await;
    let store = Store::new();
    let first = NotifyApp::start(world.config(), store.clone()).await.expect("первый экземпляр");
    let second = NotifyApp::start(world.config(), store.clone()).await.expect("второй экземпляр");
    for post in 0..60 {
        world.like(post).await;
    }
    eventually("60 уведомлений в общем хранилище", SECS, || first.unread("alice") == 60).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let (a, b) = (first.stats(), second.stats());
    assert_eq!((a.received + b.received, a.stored + b.stored), (60, 60), "каждое событие получено и сохранено ровно один раз на оба экземпляра: {a:?} {b:?}");
    assert_eq!(world.ask("unread", "alice").await, Ok(60), "запрос о непрочитанных");
    assert_eq!(world.ask("read", "alice").await, Ok(60), "отметка о прочтении");
    assert_eq!(world.ask("unread", "alice").await, Ok(0), "после неё непрочитанных нет");
    first.stop().await;
    second.stop().await;
}

#[tokio::test]
async fn a_slow_delivery_is_not_repeated() {
    let world = World::new().await;
    let sent = Arc::new(AtomicUsize::new(0));
    let counter = sent.clone();
    let deliver: Deliver = Arc::new(move |_| {
        let counter = counter.clone();
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(1400)).await;
            counter.fetch_add(1, Ordering::SeqCst);
        })
    });
    let config = Config { ack_wait: Duration::from_millis(500), deliver, ..world.config() };
    let store = Store::new();
    let first = NotifyApp::start(config.clone(), store.clone()).await.expect("первый");
    let second = NotifyApp::start(config, store.clone()).await.expect("второй");
    world.like(1).await;
    eventually("доставка закончилась", SECS, || sent.load(Ordering::SeqCst) >= 1).await;
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(sent.load(Ordering::SeqCst), 1, "доставка длилась втрое дольше ack_wait, но уведомление отправлено один раз: сервис сообщал брокеру о ходе работы");
    assert_eq!(first.unread("alice"), 1, "и в хранилище одно уведомление");
    first.stop().await;
    second.stop().await;
}
