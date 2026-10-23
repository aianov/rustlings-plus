use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use contracts::{Event, NoticeView, PostId, Topics, UserId};
use futures::StreamExt;
use nats_engine::js::{ERROR, StreamSpec, ensure_stream, publish_event};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, Meta, Pattern, Subject};
use notify_service::repository::BoxFuture;
use notify_service::{
    Audience, AudienceError, Config, Courier, MemoryNotices, Notice, NotifyService, Stats,
    controller,
};
use service_runtime::CancellationToken;

const WAIT: Duration = Duration::from_secs(10);

fn url() -> String {
    std::env::var("RUSTLINGS_NATS_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_NATS_URL.to_string())
}

fn topics() -> Topics {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    Topics::new(&format!(
        "k{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса с JetStream запущен?")
}

async fn prepare(bus: &Bus, topics: &Topics) {
    let spec = StreamSpec::new(&topics.stream(), topics.stream_subjects());
    ensure_stream(bus, &spec).await.expect("stream событий");
}

async fn drop_stream(topics: &Topics) {
    if let Ok(client) = async_nats::connect(url()).await {
        let _ = async_nats::jetstream::new(client)
            .delete_stream(topics.stream())
            .await;
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    let began = Instant::now();
    while !check() {
        assert!(began.elapsed() < WAIT, "не дождались: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn record(bus: &Bus, topics: &Topics, event: &Event, event_id: &str) {
    let words = topics.event(event);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let subject = Subject::new(&words).expect("тема");
    publish_event(bus, &subject, event_id, &event.body, &Meta::fresh())
        .await
        .expect("событие записано в stream");
}

fn like(actor: u64) -> Event {
    Event::post_liked(UserId(actor), UserId(1), PostId(7))
}

#[tokio::test]
async fn a_restart_loses_nothing_and_repeats_nothing() {
    let (bus, topics) = (bus().await, topics());
    prepare(&bus, &topics).await;
    let watch = Pattern::parse(&topics.all_notices().join(".")).expect("шаблон");
    let mut notices = bus
        .subscribe::<NoticeView>(&watch)
        .await
        .expect("подписка на уведомления");
    let config = Config {
        nats: BusConfig::new(url()),
        topics: topics.clone(),
        ..Config::default()
    };
    let stop = CancellationToken::new();
    let first = notify_service::start(None, config.clone(), stop.clone())
        .await
        .expect("первый запуск");
    for actor in 2..5 {
        record(&bus, &topics, &like(actor), &format!("like-{actor}")).await;
    }
    until("три уведомления до остановки", || first.stats().stored == 3).await;
    stop.cancel();
    first.finished().await.expect("остановился чисто");
    for actor in 5..7 {
        record(&bus, &topics, &like(actor), &format!("like-{actor}")).await;
    }
    let again = CancellationToken::new();
    let second = notify_service::start(None, config, again.clone())
        .await
        .expect("второй запуск");
    until(
        "два события, пришедшие без сервиса",
        || second.stats().stored == 2,
    )
    .await;
    let mut actors = Vec::new();
    while let Some(Ok(envelope)) = notices.next_within(Duration::from_millis(500)).await {
        actors.push(envelope.body.actor.0);
    }
    actors.sort_unstable();
    assert_eq!(
        actors,
        [2, 3, 4, 5, 6],
        "durable продолжил с места остановки: лайки, пока сервис лежал, не потерялись, старые не повторились"
    );
    assert_eq!(
        second.stats().received,
        2,
        "второй запуск не видел старых событий"
    );
    again.cancel();
    second.finished().await.expect("остановился чисто");
    drop_stream(&topics).await;
}

#[derive(Default)]
struct Postbox(Mutex<Vec<Notice>>);

impl Courier for Postbox {
    fn deliver(&self, notice: Notice, _: Option<String>) -> BoxFuture<'_, ()> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(notice);
        Box::pin(async {})
    }
}

struct Flaky {
    calls: AtomicUsize,
    outages: usize,
}

impl Audience for Flaky {
    fn readers(
        &self,
        author: UserId,
        _: &str,
    ) -> BoxFuture<'_, Result<Vec<UserId>, AudienceError>> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            match author.0 {
                9 => Err(AudienceError::Failed(String::from("нет пользователя u_9"))),
                _ if call < self.outages => Err(AudienceError::Unavailable),
                _ => Ok(vec![UserId(2), UserId(3)]),
            }
        })
    }
}

fn config(topics: &Topics) -> Config {
    Config {
        topics: topics.clone(),
        workers: 2,
        batch: 4,
        ..Config::default()
    }
}

#[tokio::test]
async fn one_event_published_twice_is_one_notice() {
    let (bus, topics) = (bus().await, topics());
    prepare(&bus, &topics).await;
    let postbox = Arc::new(Postbox::default());
    let service = Arc::new(NotifyService::new(
        Arc::new(MemoryNotices::new()),
        Arc::new(Flaky {
            calls: AtomicUsize::new(0),
            outages: 0,
        }),
        Arc::clone(&postbox) as Arc<dyn Courier>,
    ));
    let stats = Arc::new(Stats::default());
    let running = controller::start(&bus, &config(&topics), service, Arc::clone(&stats))
        .await
        .expect("контроллер");
    record(&bus, &topics, &like(2), "like-2").await;
    record(&bus, &topics, &like(2), "like-2").await;
    record(&bus, &topics, &like(2), "like-2-again").await;
    until("два события дошли", || {
        stats.snapshot().received == 2
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let seen = stats.snapshot();
    assert_eq!(
        (seen.received, seen.stored, seen.duplicates),
        (2, 1, 1),
        "повтор с тем же Nats-Msg-Id отсёк stream, а то же действие с новым id слилось в непрочитанное: {seen:?}"
    );
    running.stop().await;
    drop_stream(&topics).await;
}

#[tokio::test]
async fn a_neighbour_outage_is_retried_and_poison_is_set_aside() {
    let (bus, topics) = (bus().await, topics());
    prepare(&bus, &topics).await;
    let raw = async_nats::connect(url()).await.expect("сырое подключение");
    let mut dead = raw
        .subscribe(format!("{}.dead.>", topics.root()))
        .await
        .expect("мёртвая тема");
    raw.flush().await.expect("подписка дошла");
    let postbox = Arc::new(Postbox::default());
    let service = Arc::new(NotifyService::new(
        Arc::new(MemoryNotices::new()),
        Arc::new(Flaky {
            calls: AtomicUsize::new(0),
            outages: 2,
        }),
        Arc::clone(&postbox) as Arc<dyn Courier>,
    ));
    let stats = Arc::new(Stats::default());
    let running = controller::start(&bus, &config(&topics), service, Arc::clone(&stats))
        .await
        .expect("контроллер");
    record(
        &bus,
        &topics,
        &Event::post_created(UserId(1), PostId(7)),
        "post-7",
    )
    .await;
    until("веер дошёл после двух сбоев", || {
        stats.snapshot().stored == 2
    })
    .await;
    record(
        &bus,
        &topics,
        &Event::post_created(UserId(9), PostId(8)),
        "post-8",
    )
    .await;
    let letter = tokio::time::timeout(WAIT, dead.next())
        .await
        .expect("мёртвое письмо пришло")
        .expect("поток жив");
    let reason = letter
        .headers
        .and_then(|headers| headers.get(ERROR).map(|value| value.as_str().to_string()));
    assert_eq!(
        reason.as_deref(),
        Some("нет пользователя u_9"),
        "постоянная ошибка соседа это яд: сразу в мёртвую тему с причиной"
    );
    let seen = stats.snapshot();
    assert_eq!(
        (seen.retried, seen.handled, seen.rejected),
        (2, 1, 1),
        "временный сбой user-service вернул событие дважды с паузой, а не потерял его: {seen:?}"
    );
    assert!(seen.balanced(), "{seen:?}");
    let mut readers: Vec<UserId> = postbox
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|notice| notice.to)
        .collect();
    readers.sort();
    assert_eq!(readers, [UserId(2), UserId(3)]);
    running.stop().await;
    drop_stream(&topics).await;
}
