use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use contracts::{Origin, Topics, Username};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL};
use user_service::{MemoryAvatars, MemoryUsers, Relay, UserService};

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
        "w{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[tokio::test]
async fn a_follow_is_kept_in_the_stream_with_its_own_id() {
    let topics = topics();
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса с JetStream запущен?");
    let users = Arc::new(MemoryUsers::new());
    let relay = Relay::open(bus.clone(), topics.clone(), users.clone())
        .await
        .expect("stream событий готов");
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()));
    let name = |raw: &str| Username::try_from(raw).expect("ник");
    let alice = service
        .register(name("alice"), &Origin::new("m39"))
        .await
        .expect("alice")
        .id;
    let bob = service
        .register(name("bob"), &Origin::new("m39"))
        .await
        .expect("bob")
        .id;
    assert!(matches!(
        service.follow(bob, alice, &Origin::new("req-1")).await,
        Ok(true)
    ));
    assert_eq!(
        relay.pump().await,
        Ok(3),
        "релей выложил регистрации и подписку"
    );
    let js = async_nats::jetstream::new(async_nats::connect(url()).await.expect("подключение"));
    let mut stream = js.get_stream(topics.stream()).await.expect("stream есть");
    let info = stream.info().await.expect("сведения");
    assert_eq!(
        (info.config.name.as_str(), info.state.messages),
        (topics.stream().as_str(), 3),
        "две регистрации и подписка записаны в stream {} и ждут читателей, даже если их нет",
        topics.stream()
    );
    let mut subjects = info.config.subjects.clone();
    subjects.sort();
    assert_eq!(
        subjects,
        [
            format!("{}.dead.>", topics.root()),
            format!("{}.events.>", topics.root())
        ],
        "stream ловит события и мёртвые письма своего корня"
    );
    let record = stream.get_raw_message(3).await.expect("запись");
    let id = record
        .headers
        .get(async_nats::header::NATS_MESSAGE_ID)
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    assert!(
        id.starts_with("evt-") && id.len() > 8,
        "у события свой идентификатор для защиты от дублей: {id:?}"
    );
    Relay::open(bus, topics.clone(), users)
        .await
        .expect("второй запуск сервиса");
    let mut again = js.get_stream(topics.stream()).await.expect("stream есть");
    assert_eq!(
        again.info().await.expect("сведения").state.messages,
        3,
        "запуск сервиса не пересоздаёт stream и не стирает события"
    );
    let _ = js.delete_stream(topics.stream()).await;
}
