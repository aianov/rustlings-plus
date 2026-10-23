use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use nats_engine::js::{
    DELIVERY_COUNT, ERROR, FeedSpec, Handler, ORIGINAL_SUBJECT, Published, StreamSpec, Verdict,
    ensure_stream, open_feed, publish_event,
};
use nats_engine::{Bus, BusConfig, BusError, DEFAULT_NATS_URL, Meta, Pattern, Subject};
use serde::{Deserialize, Serialize};

const WAIT: Duration = Duration::from_secs(10);

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn root() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    format!(
        "j{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса с JetStream запущен?")
}

async fn js() -> async_nats::jetstream::Context {
    async_nats::jetstream::new(async_nats::connect(url()).await.expect("сырое подключение"))
}

async fn stored(name: &str) -> u64 {
    let mut stream = js().await.get_stream(name).await.expect("stream есть");
    stream
        .info()
        .await
        .expect("сведения о stream")
        .state
        .messages
}

async fn drop_stream(name: &str) {
    let _ = js().await.delete_stream(name).await;
}

async fn until(what: &str, check: impl Fn() -> bool) {
    let began = Instant::now();
    while !check() {
        assert!(began.elapsed() < WAIT, "не дождались: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Ev {
    n: u64,
}

struct Setup {
    bus: Bus,
    root: String,
    name: String,
}

impl Setup {
    async fn new() -> Setup {
        let (bus, root) = (bus().await, root());
        let name = format!("{}_EVENTS", root.to_uppercase());
        let spec = StreamSpec::new(
            &name,
            vec![format!("{root}.events.>"), format!("{root}.dead.>")],
        );
        ensure_stream(&bus, &spec).await.expect("stream создан");
        Setup { bus, root, name }
    }

    fn subject(&self, tail: &str) -> Subject {
        Subject::parse(&format!("{}.events.{tail}", self.root)).expect("тема")
    }

    async fn send(&self, n: u64) -> Published {
        publish_event(
            &self.bus,
            &self.subject("item.made.u_1"),
            &format!("ev-{n}"),
            &Ev { n },
            &Meta::fresh(),
        )
        .await
        .expect("событие записано")
    }

    fn spec(&self, durable: &str) -> FeedSpec {
        let filter = Pattern::parse(&format!("{}.events.>", self.root)).expect("шаблон");
        let dead = Subject::parse(&format!("{}.dead.{durable}", self.root)).expect("тема");
        FeedSpec {
            ack_wait: Duration::from_millis(500),
            batch: 8,
            backoff: vec![Duration::from_millis(150), Duration::from_millis(300)],
            ..FeedSpec::new(&self.name, durable, filter, dead)
        }
    }

    async fn finish(self) {
        drop_stream(&self.name).await;
    }
}

type Log = Arc<Mutex<Vec<(u64, Instant)>>>;

fn recording(
    log: &Log,
    decide: impl Fn(u64, usize) -> Verdict + Send + Sync + 'static,
) -> Handler<Ev> {
    let (log, decide) = (Arc::clone(log), Arc::new(decide));
    Arc::new(move |envelope| {
        let (log, decide) = (Arc::clone(&log), Arc::clone(&decide));
        Box::pin(async move {
            let n = envelope.body.n;
            let seen = {
                let mut log = log.lock().unwrap_or_else(PoisonError::into_inner);
                log.push((n, Instant::now()));
                log.iter().filter(|(m, _)| *m == n).count()
            };
            decide(n, seen)
        })
    })
}

fn numbers(log: &Log) -> Vec<u64> {
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(n, _)| *n)
        .collect()
}

#[tokio::test]
async fn ensure_stream_keeps_history_extends_subjects_and_refuses_conflicts() {
    let (bus, root) = (bus().await, root());
    let name = format!("{}_EVENTS", root.to_uppercase());
    let narrow = StreamSpec::new(&name, vec![format!("{root}.events.>")]);
    ensure_stream(&bus, &narrow).await.expect("создан");
    let subject = Subject::parse(&format!("{root}.events.a.b.u_1")).expect("тема");
    publish_event(&bus, &subject, "one", &Ev { n: 1 }, &Meta::fresh())
        .await
        .expect("записано");
    ensure_stream(&bus, &narrow)
        .await
        .expect("повтор ничего не меняет");
    assert_eq!(
        stored(&name).await,
        1,
        "повторный запуск не стирает историю"
    );
    let wide = StreamSpec::new(
        &name,
        vec![format!("{root}.events.>"), format!("{root}.dead.>")],
    );
    ensure_stream(&bus, &wide).await.expect("темы расширены");
    let mut stream = js().await.get_stream(&name).await.expect("stream");
    let info = stream.info().await.expect("сведения");
    assert_eq!(info.config.subjects.len(), 2, "новая тема добавилась");
    assert_eq!(info.state.messages, 1, "и записи на месте");
    let rival = StreamSpec::new(&format!("{name}_RIVAL"), vec![format!("{root}.events.>")]);
    assert!(
        matches!(
            ensure_stream(&bus, &rival).await,
            Err(BusError::Transport(_))
        ),
        "второй stream на те же темы брокер не даёт создать"
    );
    drop_stream(&name).await;
}

#[tokio::test]
async fn publishing_waits_for_the_ack_and_recognises_repeats_by_event_id() {
    let setup = Setup::new().await;
    let first = setup.send(1).await;
    assert_eq!(
        first,
        Published {
            sequence: 1,
            duplicate: false
        }
    );
    let again = setup.send(1).await;
    assert_eq!(
        again,
        Published {
            sequence: 1,
            duplicate: true
        },
        "тот же Nats-Msg-Id внутри окна дубликатов брокер не пишет второй раз"
    );
    assert_eq!(setup.send(2).await.sequence, 2);
    assert_eq!(stored(&setup.name).await, 2);
    let raw = js().await.get_stream(&setup.name).await.expect("stream");
    let message = raw.get_raw_message(1).await.expect("первая запись");
    let headers = message.headers;
    assert_eq!(
        headers
            .get(async_nats::header::NATS_MESSAGE_ID)
            .map(|value| value.as_str()),
        Some("ev-1"),
        "идентификатор события лежит в заголовке записи"
    );
    setup.finish().await;
}

#[tokio::test]
async fn publishing_nowhere_fails_at_once() {
    let bus = bus().await;
    let began = Instant::now();
    let nowhere = Subject::parse(&format!("{}.events.x.y.u_1", root())).expect("тема");
    let refused = publish_event(&bus, &nowhere, "x", &Ev { n: 1 }, &Meta::fresh()).await;
    assert!(
        matches!(refused, Err(BusError::Transport(_))),
        "тему не ловит ни один stream: это ошибка, а не тихая потеря: {refused:?}"
    );
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "отказ сразу, без повторов по таймауту: {:?}",
        began.elapsed()
    );
}

#[tokio::test]
async fn a_feed_handles_each_event_once_and_continues_after_a_restart() {
    let setup = Setup::new().await;
    for n in 1..=5 {
        setup.send(n).await;
    }
    let log: Log = Arc::default();
    let feed = open_feed(
        &setup.bus,
        setup.spec("reader"),
        recording(&log, |_, _| Verdict::Done),
    )
    .await
    .expect("читатель");
    until("пять событий", || numbers(&log).len() == 5).await;
    feed.stop().await;
    for n in 6..=8 {
        setup.send(n).await;
    }
    let after: Log = Arc::default();
    let again = open_feed(
        &setup.bus,
        setup.spec("reader"),
        recording(&after, |_, _| Verdict::Done),
    )
    .await
    .expect("тот же читатель после перезапуска");
    until("три новых", || numbers(&after).len() == 3).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut first = numbers(&log);
    first.sort_unstable();
    let mut second = numbers(&after);
    second.sort_unstable();
    assert_eq!(
        (first, second),
        (vec![1, 2, 3, 4, 5], vec![6, 7, 8]),
        "durable помнит, где остановился: ни потерь, ни повторов"
    );
    again.stop().await;
    setup.finish().await;
}

#[tokio::test]
async fn retry_comes_back_after_a_growing_pause() {
    let setup = Setup::new().await;
    setup.send(1).await;
    let log: Log = Arc::default();
    let feed = open_feed(
        &setup.bus,
        setup.spec("patient"),
        recording(&log, |_, seen| {
            if seen < 3 {
                Verdict::Retry
            } else {
                Verdict::Done
            }
        }),
    )
    .await
    .expect("читатель");
    until("три доставки", || numbers(&log).len() == 3).await;
    let times: Vec<Instant> = log
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(_, at)| *at)
        .collect();
    let gaps = [times[1] - times[0], times[2] - times[1]];
    assert!(
        gaps[0] >= Duration::from_millis(140) && gaps[1] >= Duration::from_millis(280),
        "пауза перед повтором растёт по backoff: {gaps:?}"
    );
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        numbers(&log).len(),
        3,
        "после Done событие больше не приходит"
    );
    feed.stop().await;
    setup.finish().await;
}

#[tokio::test]
async fn exhausted_retries_and_poison_end_in_the_dead_subject_with_the_story() {
    let setup = Setup::new().await;
    let raw = async_nats::connect(url()).await.expect("сырое подключение");
    let mut dead = raw
        .subscribe(format!("{}.dead.>", setup.root))
        .await
        .expect("подписка на мёртвую тему");
    raw.flush().await.expect("подписка дошла");
    for n in 1..=4 {
        setup.send(n).await;
    }
    let log: Log = Arc::default();
    let spec = FeedSpec {
        max_deliver: 3,
        backoff: vec![Duration::from_millis(50)],
        ..setup.spec("strict")
    };
    let feed = open_feed(
        &setup.bus,
        spec,
        recording(&log, |n, _| match n {
            1 => Verdict::Retry,
            2 => Verdict::Poison(String::from("автор\r\nнеизвестен")),
            _ => Verdict::Done,
        }),
    )
    .await
    .expect("читатель");
    let mut letters = Vec::new();
    for _ in 0..2 {
        let letter = tokio::time::timeout(WAIT, dead.next())
            .await
            .expect("мёртвое письмо пришло")
            .expect("поток жив");
        let headers = letter.headers.expect("заголовки");
        let get = |name: &str| headers.get(name).map(|value| value.as_str().to_string());
        let body: Ev = serde_json::from_slice(&letter.payload).expect("тело как было");
        letters.push((
            body.n,
            get(DELIVERY_COUNT),
            get(ERROR),
            get(ORIGINAL_SUBJECT),
        ));
    }
    letters.sort();
    let original = Some(format!("{}.events.item.made.u_1", setup.root));
    assert_eq!(
        letters,
        [
            (
                1,
                Some(String::from("3")),
                Some(String::from("исчерпаны попытки обработки")),
                original.clone()
            ),
            (
                2,
                Some(String::from("1")),
                Some(String::from("автор  неизвестен")),
                original
            ),
        ],
        "исчерпанные повторы после max_deliver и яд сразу; причина без переводов строки"
    );
    let mut handled = numbers(&log);
    handled.sort_unstable();
    assert_eq!(
        handled,
        [1, 1, 1, 2, 3, 4],
        "остальные события прошли, яд не повторяли, Retry ровно max_deliver раз"
    );
    feed.stop().await;
    setup.finish().await;
}

#[tokio::test]
async fn a_slow_handler_keeps_its_message_with_progress() {
    let setup = Setup::new().await;
    setup.send(1).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let handler: Handler<Ev> = Arc::new(move |_| {
        let counted = Arc::clone(&counted);
        Box::pin(async move {
            counted.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(1500)).await;
            Verdict::Done
        })
    });
    let feed = open_feed(&setup.bus, setup.spec("slow"), handler)
        .await
        .expect("читатель");
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "работа втрое дольше ack_wait, но «ещё работаю» не даёт брокеру отдать событие второй раз"
    );
    feed.stop().await;
    setup.finish().await;
}

#[tokio::test]
async fn two_feeds_with_one_durable_share_the_stream() {
    let setup = Setup::new().await;
    let (left, right): (Log, Log) = (Arc::default(), Arc::default());
    let spec = FeedSpec {
        batch: 2,
        ..setup.spec("shared")
    };
    let slow = |log: &Log| {
        let log = Arc::clone(log);
        let handler: Handler<Ev> = Arc::new(move |envelope| {
            let log = Arc::clone(&log);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                log.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((envelope.body.n, Instant::now()));
                Verdict::Done
            })
        });
        handler
    };
    let first = open_feed(&setup.bus, spec.clone(), slow(&left))
        .await
        .expect("первый");
    let second = open_feed(&setup.bus, spec, slow(&right))
        .await
        .expect("второй");
    for n in 1..=40 {
        setup.send(n).await;
    }
    until("40 событий на двоих", || {
        numbers(&left).len() + numbers(&right).len() >= 40
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut all = numbers(&left);
    all.extend(numbers(&right));
    all.sort_unstable();
    assert_eq!(
        all,
        (1..=40).collect::<Vec<_>>(),
        "каждое событие ровно один раз"
    );
    assert!(
        !numbers(&left).is_empty() && !numbers(&right).is_empty(),
        "работу поделили два экземпляра"
    );
    first.stop().await;
    second.stop().await;
    setup.finish().await;
}
