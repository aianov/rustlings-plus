use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use contracts::{Origin, Topics, Username};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use user_service::repository::OutboxRepository;
use user_service::{MemoryAvatars, MemoryUsers, Relay, UserService};

const WAIT: Duration = Duration::from_secs(5);

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
        "o{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

async fn js() -> async_nats::jetstream::Context {
    async_nats::jetstream::new(async_nats::connect(url()).await.expect("подключение"))
}

async fn stored(topics: &Topics) -> u64 {
    let mut stream = js()
        .await
        .get_stream(topics.stream())
        .await
        .expect("stream");
    stream.info().await.expect("сведения").state.messages
}

#[tokio::test]
async fn the_change_and_its_event_go_together() {
    let users = Arc::new(MemoryUsers::new());
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()));
    let origin = Origin::new("req-1");
    let alice = service
        .register(name("alice"), &origin)
        .await
        .expect("alice")
        .id;
    let bob = service
        .register(name("bob"), &origin)
        .await
        .expect("bob")
        .id;
    service.follow(bob, alice, &origin).await.expect("подписка");
    let rows = users.unpublished(10).await;
    assert_eq!(rows.len(), 3, "три изменения это три события в outbox");
    assert_eq!(
        rows[2].event.kind, "user.followed",
        "подписка и её событие вместе"
    );
    let through = rows.last().map(|row| row.seq).expect("строки");
    users.mark_published(through).await.expect("отметка");
    assert_eq!(users.waiting().await, 0, "отмеченное больше не ждёт");
}

#[tokio::test]
async fn the_relay_publishes_in_order_and_a_crash_before_marking_duplicates_nothing() {
    let topics = topics();
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса запущен?");
    let users = Arc::new(MemoryUsers::new());
    let relay = Relay::open(bus.clone(), topics.clone(), users.clone())
        .await
        .expect("stream готов");
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()));
    let origin = Origin::new("req-1");
    let alice = service
        .register(name("alice"), &origin)
        .await
        .expect("alice")
        .id;
    let bob = service
        .register(name("bob"), &origin)
        .await
        .expect("bob")
        .id;
    service.follow(bob, alice, &origin).await.expect("подписка");
    let ids: Vec<String> = users
        .unpublished(10)
        .await
        .into_iter()
        .map(|row| row.event.event_id)
        .collect();
    assert_eq!(relay.pump().await, Ok(3));
    assert_eq!(
        relay.pump().await,
        Ok(0),
        "опубликованное отмечено и второй раз не уходит"
    );
    let stream = js()
        .await
        .get_stream(topics.stream())
        .await
        .expect("stream");
    let mut published = Vec::new();
    for sequence in 1..=3 {
        let message = stream.get_raw_message(sequence).await.expect("запись");
        let id = message
            .headers
            .get(async_nats::header::NATS_MESSAGE_ID)
            .map(|value| value.as_str().to_string())
            .unwrap_or_default();
        published.push(id);
    }
    assert_eq!(
        published, ids,
        "события ушли в порядке outbox и с идентификатором из outbox"
    );
    assert_eq!(
        stored(&topics).await,
        3,
        "с m63 повтор после падения релея проверяет m63 на Postgres: здесь дублей нет"
    );
    let _ = js().await.delete_stream(topics.stream()).await;
}

#[tokio::test]
async fn without_the_broker_events_wait_in_the_outbox_instead_of_vanishing() {
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
    let topics = topics();
    let bus = Bus::connect(BusConfig::new(format!("nats://{addr}")))
        .await
        .expect("через прокси");
    let users = Arc::new(MemoryUsers::new());
    let relay = Relay::open(bus.clone(), topics.clone(), users.clone())
        .await
        .expect("stream готов");
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()));
    let origin = Origin::new("req-1");
    let alice = service
        .register(name("alice"), &origin)
        .await
        .expect("alice")
        .id;
    let bob = service
        .register(name("bob"), &origin)
        .await
        .expect("bob")
        .id;
    assert_eq!(relay.pump().await, Ok(2));
    cut.store(true, Ordering::SeqCst);
    severed.notify_waiters();
    let began = Instant::now();
    while bus.ready() {
        assert!(began.elapsed() < WAIT, "обрыв не заметили");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        matches!(service.follow(bob, alice, &origin).await, Ok(true)),
        "подписка записана"
    );
    assert!(
        relay.pump().await.is_err(),
        "без брокера релей не может опубликовать"
    );
    assert_eq!(
        users.waiting().await,
        1,
        "событие ждёт в outbox, а не теряется, как в m38"
    );
    cut.store(false, Ordering::SeqCst);
    let began = Instant::now();
    while !bus.ready() {
        assert!(began.elapsed() < WAIT, "связь не вернулась");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(relay.pump().await, Ok(1), "связь вернулась: релей догнал");
    assert_eq!(stored(&topics).await, 3);
    let _ = js().await.delete_stream(topics.stream()).await;
}
