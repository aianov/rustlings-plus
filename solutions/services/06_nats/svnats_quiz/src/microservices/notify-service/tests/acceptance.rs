use async_nats::Client;
use nats_engine::{Bus, BusConfig, Meta, Subject};
use notify_service::bootstrap::NotifyApp;
use notify_service::common::{Config, Deliver, Kind, Notification};
use notify_service::repository::Store;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, broadcast};

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
    raw: Client,
}

impl World {
    async fn new() -> World {
        World {
            root: unique(),
            publisher: Bus::connect(BusConfig::new(url())).await.expect("шина издателя"),
            raw: async_nats::connect(url()).await.expect("сырое подключение"),
        }
    }

    fn config(&self) -> Config {
        Config::new(url(), self.root.clone())
    }

    fn subject(&self, tail: &[&str]) -> Subject {
        let mut tokens = vec![self.root.as_str()];
        tokens.extend_from_slice(tail);
        Subject::new(&tokens).expect("тема")
    }

    async fn event(&self, tail: &[&str], body: serde_json::Value) {
        let meta = Meta::new("req-1", "trace-1").unwrap();
        self.publisher.publish(&self.subject(tail), &body, &meta).await.expect("публикация события");
    }

    async fn like(&self, user: &str, actor: &str, post: u64) {
        self.event(&["post", "liked", user], json!({"actor": actor, "post_id": post})).await;
    }

    async fn ask(&self, what: &str, user: &str) -> Result<u32, nats_engine::BusError> {
        self.publisher.request(&self.subject(&[what, user]), &json!({}), &Meta::fresh(), Duration::from_secs(2)).await
    }
}

async fn eventually(what: &str, within: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !condition() {
        assert!(Instant::now() < deadline, "за {within:?} не наступило: {what}");
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}

const SECS3: Duration = Duration::from_secs(3);

#[tokio::test]
async fn likes_comments_and_follows_become_notifications_for_the_right_user() {
    let world = World::new().await;
    let app = NotifyApp::start(world.config(), Store::new()).await.expect("запуск");
    world.like("alice", "bob", 7).await;
    world.event(&["post", "commented", "alice"], json!({"actor": "carol", "post_id": 7})).await;
    world.event(&["user", "followed", "alice"], json!({"actor": "dave"})).await;
    eventually("три уведомления у alice", SECS3, || app.unread("alice") == 3).await;
    assert_eq!(app.unread("bob"), 0, "у bob ничего нет");
    let list = app.notifications("alice");
    let kinds: Vec<Kind> = list.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, vec![Kind::Followed, Kind::Commented, Kind::Liked], "список от новых к старым: события шли в таком порядке");
    let liked: &Notification = list.iter().find(|n| n.kind == Kind::Liked).unwrap();
    assert_eq!((liked.actor.as_str(), liked.post_id, liked.read), ("bob", Some(7), false), "поля уведомления о лайке");
    assert_eq!(liked.request_id.as_deref(), Some("req-1"), "request-id события попал в уведомление");
    let followed = list.iter().find(|n| n.kind == Kind::Followed).unwrap();
    assert_eq!(followed.post_id, None, "у подписки нет поста");
    assert_eq!(world.ask("unread", "alice").await, Ok(3), "тот же счётчик по запросу NATS");
    app.stop().await;
}

#[tokio::test]
async fn what_is_not_a_notification_is_ignored_and_counted() {
    let world = World::new().await;
    let app = NotifyApp::start(world.config(), Store::new()).await.expect("запуск");
    world.event(&["post", "created", "alice"], json!({"actor": "alice", "post_id": 1})).await;
    world.like("alice", "alice", 2).await;
    world.event(&["photo", "liked", "alice"], json!({"actor": "bob", "post_id": 3})).await;
    eventually("три события разобраны", SECS3, || app.stats().received == 3).await;
    let stats = app.stats();
    assert_eq!((stats.ignored, stats.stored, stats.rejected, stats.dropped), (3, 0, 0, 0), "новый пост, свой лайк и незнакомая сущность не уведомления: {stats:?}");
    assert_eq!(app.unread("alice"), 0, "у alice по-прежнему ничего");
    app.stop().await;
}

#[tokio::test]
async fn broken_events_are_rejected_and_the_service_keeps_working() {
    let world = World::new().await;
    let app = NotifyApp::start(world.config(), Store::new()).await.expect("запуск");
    world.raw.publish(format!("{}.post.liked.alice", world.root), "это не JSON".into()).await.unwrap();
    world.event(&["post", "liked", "alice"], json!({"actor": "bad actor!", "post_id": 1})).await;
    world.event(&["post", "liked", "alice"], json!({"actor": "bob"})).await;
    world.like("alice", "bob", 5).await;
    eventually("годное событие сохранено", SECS3, || app.unread("alice") == 1).await;
    let stats = app.stats();
    assert_eq!(stats.rejected, 3, "три негодных события посчитаны как отвергнутые: {stats:?}");
    assert_eq!((stats.received, stats.stored), (4, 1), "всего четыре, сохранено одно: {stats:?}");
    app.stop().await;
}

#[tokio::test]
async fn the_same_unread_event_is_stored_once_even_when_workers_race() {
    let world = World::new().await;
    let app = NotifyApp::start(world.config(), Store::new()).await.expect("запуск");
    for _ in 0..40 {
        world.like("alice", "bob", 7).await;
    }
    eventually("все 40 событий разобраны", SECS3, || {
        let s = app.stats();
        s.stored + s.duplicates == 40
    })
    .await;
    let stats = app.stats();
    assert_eq!((stats.stored, stats.duplicates), (1, 39), "сорок одинаковых лайков до прочтения это одно уведомление: {stats:?}");
    assert_eq!(world.ask("read", "alice").await, Ok(1), "прочитано одно");
    assert_eq!(app.unread("alice"), 0, "после прочтения непрочитанных нет");
    world.like("alice", "bob", 7).await;
    eventually("лайк после прочтения снова уведомляет", SECS3, || app.unread("alice") == 1).await;
    assert_eq!(world.ask("unread", "nobody").await, Ok(0), "для незнакомого пользователя ноль, а не ошибка");
    app.stop().await;
}

#[tokio::test]
async fn two_instances_share_one_stream_and_answer_requests_once() {
    let world = World::new().await;
    let store = Store::new();
    let first = NotifyApp::start(world.config(), store.clone()).await.expect("первый экземпляр");
    let second = NotifyApp::start(world.config(), store.clone()).await.expect("второй экземпляр");
    for post in 0..60 {
        world.like("alice", "bob", post).await;
    }
    eventually("60 уведомлений в общем хранилище", SECS3, || first.unread("alice") == 60).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (a, b) = (first.stats(), second.stats());
    assert_eq!(a.stored + b.stored, 60, "каждое событие сохранено ровно один раз на оба экземпляра: {a:?} {b:?}");
    assert_eq!(a.received + b.received, 60, "и получено ровно один раз");
    assert!(a.received > 0 && b.received > 0, "работа делится между экземплярами: {a:?} {b:?}");
    for _ in 0..10 {
        assert_eq!(world.ask("unread", "alice").await, Ok(60), "любой экземпляр отвечает по общему хранилищу");
    }
    first.stop().await;
    assert_eq!(world.ask("unread", "alice").await, Ok(60), "после остановки первого отвечает второй");
    second.stop().await;
    assert_eq!(world.ask("unread", "alice").await, Err(nats_engine::BusError::NoService), "когда остановлены оба, слушателей нет");
}

struct Probe {
    entered: AtomicUsize,
    running: AtomicUsize,
    peak: AtomicUsize,
    done: AtomicUsize,
}

fn probe() -> Arc<Probe> {
    Arc::new(Probe { entered: AtomicUsize::new(0), running: AtomicUsize::new(0), peak: AtomicUsize::new(0), done: AtomicUsize::new(0) })
}

fn slow_deliver(probe: Arc<Probe>, work: Duration) -> Deliver {
    Arc::new(move |_| {
        let probe = probe.clone();
        Box::pin(async move {
            probe.entered.fetch_add(1, Ordering::SeqCst);
            let now = probe.running.fetch_add(1, Ordering::SeqCst) + 1;
            probe.peak.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(work).await;
            probe.running.fetch_sub(1, Ordering::SeqCst);
            probe.done.fetch_add(1, Ordering::SeqCst);
        })
    })
}

fn gated_deliver(probe: Arc<Probe>, gate: Arc<Semaphore>) -> Deliver {
    Arc::new(move |_| {
        let (probe, gate) = (probe.clone(), gate.clone());
        Box::pin(async move {
            probe.entered.fetch_add(1, Ordering::SeqCst);
            gate.acquire().await.expect("шлагбаум").forget();
            probe.done.fetch_add(1, Ordering::SeqCst);
        })
    })
}

#[tokio::test]
async fn a_burst_is_delivered_in_parallel_but_never_by_more_than_the_workers() {
    let world = World::new().await;
    let probe = probe();
    let config = Config { workers: 8, queue: 512, deliver: slow_deliver(probe.clone(), Duration::from_millis(20)), ..world.config() };
    let app = NotifyApp::start(config, Store::new()).await.expect("запуск");
    let started = Instant::now();
    for post in 0..200 {
        world.like("alice", "bob", post).await;
    }
    eventually("все 200 уведомлений доставлены", Duration::from_secs(3), || probe.done.load(Ordering::SeqCst) == 200).await;
    assert!(started.elapsed() < Duration::from_millis(2500), "200 доставок по 20 мс при 8 работниках не должны идти последовательно: {:?}", started.elapsed());
    let peak = probe.peak.load(Ordering::SeqCst);
    assert!(peak > 1 && peak <= 8, "одновременно доставляло {peak} задач, должно быть больше одной и не больше восьми");
    let stats = app.stats();
    assert_eq!((stats.stored, stats.dropped), (200, 0), "ничего не потеряно: {stats:?}");
    app.stop().await;
}

#[tokio::test]
async fn overload_is_shed_visibly_and_every_event_is_accounted_for() {
    let world = World::new().await;
    let probe = probe();
    let gate = Arc::new(Semaphore::new(0));
    let config = Config { workers: 2, queue: 4, deliver: gated_deliver(probe.clone(), gate.clone()), ..world.config() };
    let app = NotifyApp::start(config, Store::new()).await.expect("запуск");
    world.like("alice", "bob", 1000).await;
    world.like("alice", "bob", 1001).await;
    eventually("оба работника заняты доставкой", Duration::from_secs(3), || probe.entered.load(Ordering::SeqCst) == 2).await;
    for post in 0..50 {
        world.like("alice", "bob", post).await;
    }
    eventually("приём разобрал все события, пока работники стоят", Duration::from_secs(3), || app.stats().received == 52).await;
    let stats = app.stats();
    assert_eq!((stats.queued, stats.dropped), (6, 46), "приняты 2 в работу и 4 в очередь, остальные 46 выброшены и посчитаны: {stats:?}");
    assert_eq!(stats.received, stats.ignored + stats.rejected + stats.queued + stats.dropped, "каждое полученное событие учтено: {stats:?}");
    gate.add_permits(1000);
    eventually("принятые обработаны", Duration::from_secs(3), || probe.done.load(Ordering::SeqCst) == 6).await;
    assert_eq!(app.stats().stored, 6, "сохранено ровно то, что приняли: 2 в работе и 4 в очереди");
    app.stop().await;
}

#[tokio::test]
async fn stop_finishes_the_queued_work_before_returning() {
    let world = World::new().await;
    let probe = probe();
    let config = Config { workers: 2, queue: 100, deliver: slow_deliver(probe.clone(), Duration::from_millis(10)), ..world.config() };
    let app = NotifyApp::start(config, Store::new()).await.expect("запуск");
    for post in 0..60 {
        world.like("alice", "bob", post).await;
    }
    eventually("все 60 событий приняты", Duration::from_secs(3), || app.stats().queued == 60).await;
    let store_view = app.stats();
    assert!(store_view.stored < 60, "тест должен останавливать сервис с непустой очередью, а там уже {store_view:?}");
    tokio::time::timeout(Duration::from_secs(5), async { app.stop().await }).await.expect("остановка зависла");
    assert_eq!(probe.done.load(Ordering::SeqCst), 60, "stop вернулся только после доставки всего, что уже было в очереди");
}

#[tokio::test]
async fn readiness_follows_the_broker_and_the_service_recovers() {
    let proxy = Proxy::start().await;
    let world = World::new().await;
    let config = Config { url: proxy.url(), ..world.config() };
    let app = NotifyApp::start(config, Store::new()).await.expect("запуск");
    assert!(app.ready(), "после запуска готов");
    proxy.cut();
    eventually("готовность снята после обрыва", Duration::from_secs(2), || !app.ready()).await;
    assert_eq!(app.unread("alice"), 0, "чтение собственного хранилища связи не требует");
    proxy.restore();
    eventually("готовность вернулась", Duration::from_secs(6), || app.ready()).await;
    world.like("alice", "bob", 1).await;
    eventually("после восстановления события снова обрабатываются", SECS3, || app.unread("alice") == 1).await;
    app.stop().await;
}

struct Proxy {
    addr: SocketAddr,
    cut: Arc<AtomicBool>,
    kill: broadcast::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl Proxy {
    async fn start() -> Proxy {
        let upstream = url();
        let upstream = upstream.strip_prefix("nats://").unwrap_or(&upstream).to_string();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cut = Arc::new(AtomicBool::new(false));
        let (kill, _) = broadcast::channel(1);
        let task = tokio::spawn({
            let (cut, kill) = (cut.clone(), kill.clone());
            async move {
                while let Ok((client, _)) = listener.accept().await {
                    if cut.load(Ordering::SeqCst) {
                        continue;
                    }
                    let (mut stop, upstream) = (kill.subscribe(), upstream.clone());
                    tokio::spawn(async move {
                        let Ok(server) = TcpStream::connect(&upstream).await else { return };
                        let (mut client, mut server) = (client, server);
                        tokio::select! {
                            _ = tokio::io::copy_bidirectional(&mut client, &mut server) => {}
                            _ = stop.recv() => {}
                        }
                    });
                }
            }
        });
        Proxy { addr, cut, kill, task }
    }

    fn url(&self) -> String {
        format!("nats://{}", self.addr)
    }

    fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        self.kill.send(()).ok();
    }

    fn restore(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
        self.kill.send(()).ok();
    }
}
