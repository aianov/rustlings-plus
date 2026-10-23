use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use contracts::{Event, EventBody, Origin, ROOT_ENV, Topics, UserId, Username};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, NATS_URL_ENV, Pattern};
use service_runtime::Source;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use user_service::repository::BoxFuture;
use user_service::{BusEvents, Config, Events, MemoryAvatars, MemoryUsers, UserError, UserService};

const WAIT: Duration = Duration::from_secs(3);

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
        "u{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn drop_stream(topics: &Topics) {
    if let Ok(client) = async_nats::connect(url()).await {
        let _ = async_nats::jetstream::new(client)
            .delete_stream(topics.stream())
            .await;
    }
}

#[derive(Default)]
struct Recorded(Mutex<Vec<(Event, Origin)>>);

impl Recorded {
    fn take(&self) -> Vec<(Event, Origin)> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Events for Recorded {
    fn publish(&self, event: Event, origin: Origin) -> BoxFuture<'_, ()> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((event, origin));
        Box::pin(async {})
    }
}

async fn people(service: &UserService) -> (UserId, UserId) {
    let name = |raw: &str| Username::try_from(raw).expect("ник");
    let alice = service.register(name("alice")).await.expect("alice");
    let bob = service.register(name("bob")).await.expect("bob");
    (alice.id, bob.id)
}

fn service(events: Arc<dyn Events>) -> UserService {
    UserService::new(
        Arc::new(MemoryUsers::new()),
        Arc::new(MemoryAvatars::default()),
    )
    .with_events(events)
}

#[tokio::test]
async fn a_follow_is_published_once_and_only_when_something_changed() {
    let recorded = Arc::new(Recorded::default());
    let service = service(Arc::clone(&recorded) as Arc<dyn Events>);
    let (alice, bob) = people(&service).await;
    let origin = Origin::new("e856e880e9a85e6d").traced(Some("trace-7"));
    assert!(matches!(
        service.follow(bob, alice, &origin).await,
        Ok(true)
    ));
    assert_eq!(
        recorded.take(),
        [(Event::followed(bob, alice), origin.clone())],
        "подписка случилась: одно событие user.followed про alice с номером запроса"
    );
    assert!(matches!(
        service.follow(bob, alice, &origin).await,
        Ok(false)
    ));
    assert!(matches!(
        service.follow(alice, alice, &origin).await,
        Err(UserError::SelfFollow)
    ));
    assert!(matches!(
        service.follow(bob, UserId(99), &origin).await,
        Err(UserError::UnknownUser(_))
    ));
    assert_eq!(
        recorded.take(),
        [],
        "повтор, подписка на себя и на несуществующего ничего не меняют и событий не дают"
    );
}

#[tokio::test]
async fn the_bus_adapter_puts_the_fact_on_its_subject() {
    let topics = topics();
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса запущен?");
    let watch = Pattern::parse(&topics.all_events().join(".")).expect("шаблон");
    let mut seen = bus.subscribe::<EventBody>(&watch).await.expect("подписка");
    let events = BusEvents::open(bus.clone(), topics.clone())
        .await
        .expect("stream событий готов");
    let origin = Origin::new("e856e880e9a85e6d").traced(Some("trace-7"));
    events
        .publish(Event::followed(UserId(2), UserId(1)), origin)
        .await;
    let got = seen
        .next_within(WAIT)
        .await
        .expect("событие пришло")
        .expect("событие разобралось");
    assert_eq!(
        got.subject.as_str(),
        format!("{}.events.user.followed.u_1", topics.root())
    );
    assert_eq!(
        got.body,
        EventBody {
            actor: UserId(2),
            post: None
        }
    );
    let meta = got.meta.expect("заголовки");
    assert_eq!(
        (meta.request_id.as_str(), meta.trace_id.as_str()),
        ("e856e880e9a85e6d", "trace-7"),
        "номер запроса шлюза и трасса едут заголовками события"
    );
    drop_stream(&topics).await;
}

#[tokio::test]
async fn a_follow_succeeds_while_the_broker_is_away() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr: SocketAddr = listener.local_addr().expect("адрес");
    let cut = Arc::new(AtomicBool::new(false));
    let severed = Arc::new(Notify::new());
    let target = url().trim_start_matches("nats://").to_string();
    let (refuse, kill) = (Arc::clone(&cut), Arc::clone(&severed));
    tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            if refuse.load(Ordering::SeqCst) {
                continue;
            }
            let Ok(mut server) = TcpStream::connect(&target).await else {
                continue;
            };
            let kill = Arc::clone(&kill);
            tokio::spawn(async move {
                tokio::select! {
                    _ = copy_bidirectional(&mut client, &mut server) => {}
                    () = kill.notified() => {}
                }
            });
        }
    });
    let bus = Bus::connect(BusConfig::new(format!("nats://{addr}")))
        .await
        .expect("через прокси");
    let topics = topics();
    let events = BusEvents::open(bus.clone(), topics.clone())
        .await
        .expect("stream событий готов");
    let service = service(Arc::new(events));
    let (alice, bob) = people(&service).await;
    cut.store(true, Ordering::SeqCst);
    severed.notify_waiters();
    let began = Instant::now();
    while bus.ready() {
        assert!(began.elapsed() < WAIT, "обрыв не заметили");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let began = Instant::now();
    let origin = Origin::new("e856e880e9a85e6d");
    assert!(
        matches!(service.follow(bob, alice, &origin).await, Ok(true)),
        "подписка записана, хотя событие потеряно: брокер уведомлений не держит соцсеть"
    );
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "без брокера публикация отказывает сразу, а не ждёт: {:?}",
        began.elapsed()
    );
    drop_stream(&topics).await;
}

#[test]
fn the_broker_and_the_root_come_from_the_environment() {
    let config = Config::from_source(&Source::from_pairs([
        (NATS_URL_ENV, "nats://127.0.0.1:4999"),
        (ROOT_ENV, "social_dev"),
    ]))
    .expect("годный конфиг");
    assert_eq!(
        config.nats.map(|nats| (nats.url, nats.name)),
        Some((
            String::from("nats://127.0.0.1:4999"),
            Some(String::from("user-service"))
        )),
        "адрес из NATS_URL, имя соединения это имя сервиса"
    );
    assert_eq!(config.topics.root(), "social_dev");
    let quiet = Config::from_source(&Source::from_pairs([(ROOT_ENV, "social")]))
        .expect("без NATS_URL сервис работает без событий");
    assert_eq!(quiet.nats, None);
    let crooked = Config::from_source(&Source::from_pairs([(ROOT_ENV, "social.>")]))
        .expect_err("корень с точкой и подстановкой");
    assert_eq!(crooked.keys(), [ROOT_ENV], "{crooked}");
}
