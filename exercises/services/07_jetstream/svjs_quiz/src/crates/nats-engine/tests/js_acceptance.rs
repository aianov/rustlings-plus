use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::stream::{Config, RetentionPolicy};
use async_nats::jetstream::{self, Context};
use futures::StreamExt;
use nats_engine::js::{self, Feed, FeedSpec, Handler, MAX_DELIVER, StreamSpec, Verdict};
use nats_engine::{Bus, BusConfig, BusError, Envelope, Meta, Pattern, Seg, Subject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

const DEFAULT_URL: &str = "nats://127.0.0.1:54222";

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

fn unique() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    format!("t_{}_{nanos:x}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Ev {
    n: u64,
}

struct World {
    root: String,
    bus: Bus,
    js: Context,
    stream: String,
}

impl Drop for World {
    fn drop(&mut self) {
        let (url, stream) = (url(), self.stream.clone());
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
        let root = unique();
        let bus = Bus::connect(BusConfig::new(url())).await.expect("шина");
        let js = jetstream::new(async_nats::connect(url()).await.expect("подключение"));
        World { stream: format!("{root}_EVENTS"), root, bus, js }
    }

    fn spec(&self) -> StreamSpec {
        StreamSpec {
            name: self.stream.clone(),
            subjects: vec![format!("{}.ev.>", self.root), format!("{}.dead.>", self.root)],
            duplicate_window: Duration::from_secs(60),
        }
    }

    fn ev(&self, tail: &str) -> Subject {
        Subject::new(&[self.root.as_str(), "ev", tail]).unwrap()
    }

    fn dead(&self) -> Subject {
        Subject::new(&[self.root.as_str(), "dead", "all"]).unwrap()
    }

    fn feed_spec(&self, durable: &str, ack_wait: Duration) -> FeedSpec {
        FeedSpec {
            stream: self.stream.clone(),
            durable: durable.to_string(),
            filter: Pattern::build(&[Seg::Word(&self.root), Seg::Word("ev"), Seg::Tail]).unwrap(),
            ack_wait,
            batch: 10,
            dead_subject: self.dead(),
        }
    }

    async fn publish(&self, id: &str, n: u64) -> js::Published {
        js::publish_event(&self.bus, &self.ev("x"), id, &Ev { n }, &Meta::fresh()).await.expect("публикация")
    }

    async fn dead_letters(&self) -> Vec<(String, Vec<(String, String)>)> {
        let stream = self.js.get_stream(&self.stream).await.unwrap();
        let consumer = stream
            .create_consumer(pull::Config { ack_policy: AckPolicy::None, filter_subject: format!("{}.dead.>", self.root), ..Default::default() })
            .await
            .unwrap();
        let mut batch = consumer.fetch().max_messages(50).expires(Duration::from_millis(300)).messages().await.unwrap();
        let mut out = Vec::new();
        while let Some(message) = batch.next().await {
            let message = message.unwrap();
            let headers = message
                .headers
                .as_ref()
                .map(|h| h.iter().map(|(k, v)| (k.to_string(), v.iter().map(|x| x.as_str().to_string()).collect::<Vec<_>>().join(","))).collect())
                .unwrap_or_default();
            out.push((String::from_utf8_lossy(&message.payload).into_owned(), headers));
        }
        out
    }
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

type Log = Arc<Mutex<Vec<(u64, Instant)>>>;

fn recording(log: &Log, decide: impl Fn(u64, usize) -> Verdict + Send + Sync + 'static, work: Duration) -> Handler<Ev> {
    let log = log.clone();
    let decide = Arc::new(decide);
    Arc::new(move |envelope: Envelope<Ev>| {
        let (log, decide) = (log.clone(), decide.clone());
        Box::pin(async move {
            let attempt = {
                let mut log = log.lock().unwrap();
                log.push((envelope.body.n, Instant::now()));
                log.iter().filter(|(n, _)| *n == envelope.body.n).count()
            };
            if !work.is_zero() {
                tokio::time::sleep(work).await;
            }
            decide(envelope.body.n, attempt)
        })
    })
}

async fn wait_for(what: &str, within: Duration, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !ready() {
        assert!(Instant::now() < deadline, "за {within:?} не наступило: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn numbers(log: &Log) -> Vec<u64> {
    log.lock().unwrap().iter().map(|(n, _)| *n).collect()
}

const SECS: Duration = Duration::from_secs(5);

#[tokio::test]
async fn ensure_stream_is_idempotent_extends_subjects_and_refuses_conflicts() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.expect("создание");
    world.publish("a", 1).await;
    world.publish("b", 2).await;
    js::ensure_stream(&world.bus, &world.spec()).await.expect("повторный вызов");
    let mut stream = world.js.get_stream(&world.stream).await.unwrap();
    assert_eq!(stream.info().await.unwrap().state.messages, 2, "повторный запуск не стирает историю");
    let mut wider = world.spec();
    wider.subjects.push(format!("{}.extra.>", world.root));
    js::ensure_stream(&world.bus, &wider).await.expect("добавили тему");
    let info = world.js.get_stream(&world.stream).await.unwrap().get_info().await.unwrap();
    assert_eq!((info.config.subjects.len(), info.state.messages), (3, 2), "тема добавилась, записи на месте");

    let other = World::new().await;
    other
        .js
        .create_stream(Config { name: other.stream.clone(), subjects: vec![format!("{}.ev.>", other.root)], retention: RetentionPolicy::WorkQueue, ..Default::default() })
        .await
        .unwrap();
    let result = js::ensure_stream(&other.bus, &other.spec()).await;
    assert!(result.is_err(), "темы нужно менять, а stream другого вида (очередь заданий) этого не позволяет: это ошибка запуска");
}

#[tokio::test]
async fn publishing_waits_for_the_ack_and_recognises_repeats_by_event_id() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    let first = world.publish("like-7-1", 7).await;
    let again = world.publish("like-7-1", 7).await;
    let other = world.publish("like-7-2", 8).await;
    assert_eq!(first, js::Published { sequence: 1, duplicate: false }, "первая публикация");
    assert_eq!(again, js::Published { sequence: 1, duplicate: true }, "повтор того же события это дубликат с тем же номером");
    assert_eq!(other, js::Published { sequence: 2, duplicate: false }, "другое событие получает следующий номер");
    let uncovered = Subject::new(&[world.root.as_str(), "nowhere", "x"]).unwrap();
    let result = js::publish_event(&world.bus, &uncovered, "z", &Ev { n: 0 }, &Meta::fresh()).await;
    assert!(matches!(result, Err(BusError::Transport(_))), "тему не покрывает ни один stream: ошибка, а не тишина, получили {result:?}");
}

#[tokio::test]
async fn publishing_without_a_connection_fails_at_once() {
    let proxy = Proxy::start().await;
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    let bus = Bus::connect(BusConfig::new(proxy.url())).await.unwrap();
    proxy.cut();
    let deadline = Instant::now() + Duration::from_secs(2);
    while bus.ready() {
        assert!(Instant::now() < deadline, "обрыв не замечен");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let started = Instant::now();
    let result = js::publish_event(&bus, &world.ev("x"), "id", &Ev { n: 1 }, &Meta::fresh()).await;
    assert_eq!(result, Err(BusError::Unavailable), "без связи сразу Unavailable");
    assert!(started.elapsed() < Duration::from_millis(500), "и без ожидания таймаутов");
}

#[tokio::test]
async fn a_feed_handles_each_event_once_and_a_restart_continues_from_the_same_place() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    for n in 1..=12 {
        world.publish(&format!("e{n}"), n).await;
    }
    let log: Log = Arc::default();
    let feed = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_secs(5)), recording(&log, |_, _| Verdict::Done, Duration::ZERO)).await.expect("feed");
    wait_for("12 событий обработаны", SECS, || log.lock().unwrap().len() == 12).await;
    assert_eq!(numbers(&log), (1..=12).collect::<Vec<_>>(), "по порядку и по разу");
    feed.stop().await;
    for n in 13..=15 {
        world.publish(&format!("e{n}"), n).await;
    }
    let log2: Log = Arc::default();
    let restarted = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_secs(5)), recording(&log2, |_, _| Verdict::Done, Duration::ZERO)).await.unwrap();
    wait_for("три новых события", SECS, || log2.lock().unwrap().len() >= 3).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(numbers(&log2), vec![13, 14, 15], "после рестарта только новые события: историю заново не читают");
    restarted.stop().await;
}

#[tokio::test]
async fn retry_comes_back_after_a_growing_pause_and_then_succeeds() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    world.publish("e1", 1).await;
    let log: Log = Arc::default();
    let handler = recording(&log, |_, attempt| if attempt < 3 { Verdict::Retry } else { Verdict::Done }, Duration::ZERO);
    let feed = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_secs(5)), handler).await.unwrap();
    wait_for("три попытки", SECS, || log.lock().unwrap().len() >= 3).await;
    tokio::time::sleep(Duration::from_millis(900)).await;
    let times: Vec<Instant> = log.lock().unwrap().iter().map(|(_, t)| *t).collect();
    assert_eq!(times.len(), 3, "после успешной третьей попытки событие больше не приходит");
    let gaps: Vec<u128> = times.windows(2).map(|w| (w[1] - w[0]).as_millis()).collect();
    assert!(gaps[0] >= 80 && gaps[1] >= 180, "паузы между повторами растут (100 мс, потом 200 мс): {gaps:?}");
    assert!(world.dead_letters().await.is_empty(), "событие в итоге обработано: мёртвая тема пуста");
    feed.stop().await;
}

#[tokio::test]
async fn exhausted_retries_end_in_the_dead_subject_with_the_story() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    world.publish("bad", 99).await;
    world.publish("good", 1).await;
    let log: Log = Arc::default();
    let handler = recording(&log, |n, _| if n == 99 { Verdict::Retry } else { Verdict::Done }, Duration::ZERO);
    let feed = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_secs(5)), handler).await.unwrap();
    wait_for("все попытки израсходованы", Duration::from_secs(8), || log.lock().unwrap().iter().filter(|(n, _)| *n == 99).count() as i64 == MAX_DELIVER).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(log.lock().unwrap().iter().filter(|(n, _)| *n == 99).count() as i64, MAX_DELIVER, "ровно MAX_DELIVER попыток и ни одной больше");
    assert_eq!(log.lock().unwrap().iter().filter(|(n, _)| *n == 1).count(), 1, "хорошее событие обработано один раз и не ждало плохое");
    let dead = world.dead_letters().await;
    assert_eq!(dead.len(), 1, "в мёртвой теме одна запись: {dead:?}");
    assert_eq!(dead[0].0, r#"{"n":99}"#, "исходное тело");
    assert_eq!(header(&dead[0].1, "X-Delivery-Count"), Some(MAX_DELIVER.to_string().as_str()), "сколько раз пробовали");
    assert!(header(&dead[0].1, "X-Original-Subject").is_some_and(|s| s.ends_with(".ev.x")), "откуда пришло");
    assert!(header(&dead[0].1, "X-Error").is_some_and(|s| !s.is_empty()), "причина");
    feed.stop().await;
}

#[tokio::test]
async fn poison_and_garbage_are_set_aside_at_once_while_the_rest_flows() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    let raw = async_nats::connect(url()).await.unwrap();
    let raw_js = jetstream::new(raw);
    raw_js.publish(format!("{}.ev.x", world.root), "это не JSON".into()).await.unwrap().await.unwrap();
    world.publish("p", 13).await;
    world.publish("ok", 2).await;
    let log: Log = Arc::default();
    let handler = recording(&log, |n, _| if n == 13 { Verdict::Poison("число 13 запрещено".to_string()) } else { Verdict::Done }, Duration::ZERO);
    let feed = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_secs(5)), handler).await.unwrap();
    wait_for("хорошее событие обработано", SECS, || numbers(&log).contains(&2)).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(numbers(&log), vec![13, 2], "мусор в обработчик не попал, ядовитое событие пробовали один раз, хорошее обработано");
    let dead = world.dead_letters().await;
    assert_eq!(dead.len(), 2, "в мёртвой теме обе записи: {dead:?}");
    let garbage = dead.iter().find(|(payload, _)| payload == "это не JSON").expect("мусор в мёртвой теме");
    assert!(header(&garbage.1, "X-Error").is_some_and(|s| !s.is_empty()), "причина для мусора");
    let poison = dead.iter().find(|(payload, _)| payload.contains("13")).expect("ядовитое в мёртвой теме");
    assert_eq!(header(&poison.1, "X-Error"), Some("число 13 запрещено"), "причина от обработчика");
    assert_eq!(header(&poison.1, "X-Delivery-Count"), Some("1"), "попытка была одна");
    feed.stop().await;
}

#[tokio::test]
async fn a_slow_handler_is_not_duplicated_because_the_feed_reports_progress() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    world.publish("slow", 5).await;
    let log: Log = Arc::default();
    let first = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_millis(500)), recording(&log, |_, _| Verdict::Done, Duration::from_millis(1700))).await.unwrap();
    let second_bus = Bus::connect(BusConfig::new(url())).await.unwrap();
    let second = js::open_feed(&second_bus, world.feed_spec("notify", Duration::from_millis(500)), recording(&log, |_, _| Verdict::Done, Duration::from_millis(1700))).await.unwrap();
    wait_for("обработчик отработал", Duration::from_secs(6), || !log.lock().unwrap().is_empty()).await;
    tokio::time::sleep(Duration::from_millis(2600)).await;
    assert_eq!(log.lock().unwrap().len(), 1, "пока один экземпляр работает 1,7 с при ack_wait 0,5 с, брокер не отдаёт то же событие второму");
    first.stop().await;
    second.stop().await;
}

#[tokio::test]
async fn two_feeds_with_one_durable_share_the_stream() {
    let world = World::new().await;
    js::ensure_stream(&world.bus, &world.spec()).await.unwrap();
    for n in 1..=40 {
        world.publish(&format!("e{n}"), n).await;
    }
    let (log_a, log_b): (Log, Log) = (Arc::default(), Arc::default());
    let other_bus = Bus::connect(BusConfig::new(url())).await.unwrap();
    let a: Feed = js::open_feed(&world.bus, world.feed_spec("notify", Duration::from_secs(5)), recording(&log_a, |_, _| Verdict::Done, Duration::from_millis(5))).await.unwrap();
    let b: Feed = js::open_feed(&other_bus, world.feed_spec("notify", Duration::from_secs(5)), recording(&log_b, |_, _| Verdict::Done, Duration::from_millis(5))).await.unwrap();
    wait_for("40 событий обработаны", Duration::from_secs(8), || log_a.lock().unwrap().len() + log_b.lock().unwrap().len() >= 40).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let mut all = numbers(&log_a);
    all.extend(numbers(&log_b));
    let unique: BTreeSet<u64> = all.iter().copied().collect();
    assert_eq!((all.len(), unique.len()), (40, 40), "каждое из 40 событий обработано ровно один раз на два экземпляра");
    a.stop().await;
    b.stop().await;
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
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
        self.kill.send(()).ok();
    }
}
