use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use contracts::{
    DomainEvent, Fact, Followed, Muted, PostCreated, PostDeleted, PostEdited, PostId, PostLiked,
    ProfileChanged, Registered, Topics, UserId, fresh_event_id,
};
use feed_service::{Config, Consumed, FeedService, MemoryFeed};
use grpc_kit::Stamp;
use nats_engine::js::{StreamSpec, ensure_stream, publish_event};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, Meta, Subject};
use proto::social::v1::FeedRequest;
use proto::social::v1::feeds_client::FeedsClient;
use serde_json::json;
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tonic::transport::Channel;

const WAIT: Duration = Duration::from_secs(10);

fn at(version: u64, fact: Fact, when: u64) -> DomainEvent {
    DomainEvent::new(&fresh_event_id(), &fact, version, when)
}

fn registered(user: u64, name: &str) -> DomainEvent {
    at(
        1,
        Fact::Registered(Registered {
            user_id: UserId(user),
            username: name.to_string(),
        }),
        1,
    )
}

fn followed(version: u64, follower: u64, followee: u64) -> DomainEvent {
    at(
        version,
        Fact::Followed(Followed {
            follower_id: UserId(follower),
            followee_id: UserId(followee),
        }),
        2,
    )
}

fn posted(post: u64, author: u64, text: &str, when: u64) -> DomainEvent {
    at(
        1,
        Fact::PostCreated(PostCreated {
            post_id: PostId(post),
            author_id: UserId(author),
            author_name: format!("ник{author}"),
            text: text.to_string(),
            tags: Vec::new(),
            links: Vec::new(),
        }),
        when,
    )
}

fn liked(version: u64, post: u64, author: u64, user: u64) -> DomainEvent {
    at(
        version,
        Fact::PostLiked(PostLiked {
            post_id: PostId(post),
            author_id: UserId(author),
            user_id: UserId(user),
        }),
        9,
    )
}

fn world() -> Vec<DomainEvent> {
    vec![
        registered(1, "alice"),
        registered(2, "bob"),
        registered(3, "carol"),
        followed(2, 2, 1),
        followed(3, 2, 3),
        posted(1, 1, "первый", 100),
        posted(2, 3, "от carol", 200),
        posted(3, 1, "второй", 300),
        liked(2, 1, 1, 2),
        liked(3, 1, 1, 3),
    ]
}

async fn shown(service: &FeedService, viewer: u64) -> Vec<(u64, String, String, usize)> {
    service
        .page(UserId(viewer), 1, 10)
        .await
        .into_iter()
        .map(|item| (item.post.0, item.author_name, item.text, item.likes))
        .collect()
}

async fn feed_of(events: &[DomainEvent]) -> FeedService {
    let service = FeedService::new(Arc::new(MemoryFeed::new()));
    for event in events {
        service.consume(event).await.expect("событие разобрано");
    }
    service
}

#[tokio::test]
async fn the_feed_is_built_from_events() {
    let service = feed_of(&world()).await;
    assert_eq!(
        shown(&service, 2).await,
        [
            (3, String::from("alice"), String::from("второй"), 0),
            (2, String::from("carol"), String::from("от carol"), 0),
            (1, String::from("alice"), String::from("первый"), 2),
        ],
        "лента bob: посты тех, на кого он подписан, новые первыми по времени события, лайки посчитаны"
    );
    assert_eq!(
        shown(&service, 1).await.len(),
        2,
        "alice ни на кого не подписана и видит только свои посты"
    );
    assert_eq!(
        service.page(UserId(2), 2, 2).await.len(),
        1,
        "вторая страница по 2"
    );
}

#[tokio::test]
async fn any_order_and_repeats_give_the_same_projection() {
    let reference = feed_of(&world()).await.snapshot().await;
    let mut backwards = world();
    backwards.reverse();
    let mut doubled = world();
    doubled.extend(world());
    let mut shifted = world();
    shifted.rotate_left(4);
    for (name, events) in [
        ("задом наперёд", backwards),
        ("дважды", doubled),
        ("со сдвигом", shifted),
    ] {
        let service = FeedService::new(Arc::new(MemoryFeed::new()));
        for event in &events {
            service.consume(event).await.expect("событие разобрано");
        }
        let snapshot = service.snapshot().await;
        assert_eq!(snapshot, reference, "{name}: проекция та же");
        assert_eq!(snapshot.waiting, 0, "{name}: ничего не застряло в ожидании");
    }
}

#[tokio::test]
async fn a_future_version_waits_and_a_repeat_is_skipped() {
    let service = FeedService::new(Arc::new(MemoryFeed::new()));
    let first = posted(1, 1, "пост", 1);
    let third = liked(3, 1, 1, 3);
    assert_eq!(
        service.consume(&third).await,
        Ok(Consumed::Buffered),
        "v3 раньше v2 ждёт"
    );
    assert_eq!(
        service.consume(&third).await,
        Ok(Consumed::Duplicate),
        "ждущее не задваивается"
    );
    assert_eq!(service.consume(&first).await, Ok(Consumed::Applied));
    assert_eq!(
        service.consume(&liked(2, 1, 1, 2)).await,
        Ok(Consumed::Applied),
        "v2 открывает v3"
    );
    assert_eq!(
        service.consume(&first).await,
        Ok(Consumed::Duplicate),
        "уже применённое пропускается"
    );
    let posts = service.snapshot().await.posts;
    assert_eq!(
        posts.get(&PostId(1)).map(|post| post.2),
        Some(2),
        "оба лайка ровно по разу"
    );
}

#[tokio::test]
async fn a_new_display_name_shows_on_old_posts_even_from_schema_one() {
    let mut events = world();
    events.push(at(
        2,
        Fact::ProfileChanged(ProfileChanged {
            user_id: UserId(1),
            display_name: Some(String::from("Алиса")),
        }),
        400,
    ));
    let service = feed_of(&events).await;
    let names: Vec<String> = shown(&service, 2)
        .await
        .into_iter()
        .map(|row| row.1)
        .collect();
    assert_eq!(
        names,
        ["Алиса", "carol", "Алиса"],
        "имя берётся на момент чтения, и в старых постах тоже"
    );
    let old = DomainEvent {
        event_id: fresh_event_id(),
        kind: String::from("user.profile_changed"),
        schema: 1,
        version: 2,
        occurred_at: 400,
        aggregate_id: String::from("u_3"),
        payload: json!({"name": "Кэрол"}),
    };
    service
        .consume(&old)
        .await
        .expect("событие старой схемы поднимается апкастом");
    let names: Vec<String> = shown(&service, 2)
        .await
        .into_iter()
        .map(|row| row.1)
        .collect();
    assert_eq!(names, ["Алиса", "Кэрол", "Алиса"]);
}

#[tokio::test]
async fn mutes_edits_and_deletes_change_what_is_shown() {
    let mut events = world();
    events.push(at(
        4,
        Fact::Muted(Muted {
            user_id: UserId(2),
            muted_id: UserId(3),
        }),
        500,
    ));
    events.push(at(
        4,
        Fact::PostEdited(PostEdited {
            post_id: PostId(1),
            text: String::from("исправлено"),
        }),
        500,
    ));
    events.push(at(
        2,
        Fact::PostDeleted(PostDeleted { post_id: PostId(3) }),
        500,
    ));
    let service = feed_of(&events).await;
    assert_eq!(
        shown(&service, 2).await,
        [(1, String::from("alice"), String::from("исправлено"), 2)],
        "скрытый автор и удалённый пост уходят, правка видна"
    );
}

#[tokio::test]
async fn an_unknown_event_still_moves_the_version() {
    let service = FeedService::new(Arc::new(MemoryFeed::new()));
    service
        .consume(&posted(1, 1, "пост", 1))
        .await
        .expect("пост");
    let mut pinned = liked(2, 1, 1, 2);
    pinned.kind = String::from("post.pinned");
    assert_eq!(
        service.consume(&pinned).await,
        Ok(Consumed::Applied),
        "незнакомое событие без эффекта"
    );
    assert_eq!(
        service.consume(&liked(3, 1, 1, 2)).await,
        Ok(Consumed::Applied),
        "следующая версия не ждёт вечно: незнакомое событие тоже сдвинуло счётчик агрегата"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn one_event_delivered_eight_times_at_once_counts_once() {
    let service = Arc::new(feed_of(&[posted(1, 1, "пост", 1)]).await);
    let like = liked(2, 1, 1, 2);
    let mut racers = Vec::new();
    for _ in 0..8 {
        let (service, like) = (Arc::clone(&service), like.clone());
        racers.push(tokio::spawn(async move { service.consume(&like).await }));
    }
    let mut applied = 0;
    for racer in racers {
        if racer.await.expect("задача") == Ok(Consumed::Applied) {
            applied += 1;
        }
    }
    assert_eq!(applied, 1, "отметка версии и эффект под одним замком");
    let posts = service.snapshot().await.posts;
    assert_eq!(posts.get(&PostId(1)).map(|post| post.2), Some(1));
}

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
        "f{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn start(
    topics: &Topics,
) -> (
    String,
    CancellationToken,
    tokio::task::JoinHandle<feed_service::Result<()>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    let config = Config {
        nats: BusConfig::new(url()),
        topics: topics.clone(),
        ..Config::default()
    };
    let stop = CancellationToken::new();
    let task = tokio::spawn(feed_service::run(listener, config, stop.clone()));
    (addr, stop, task)
}

async fn page_of(addr: &str, viewer: u64) -> Vec<(u64, String, u64)> {
    let channel = Channel::from_shared(format!("http://{addr}"))
        .expect("адрес")
        .connect()
        .await
        .expect("feed-service отвечает");
    let mut feeds = FeedsClient::with_interceptor(channel, Stamp::fresh().user(viewer));
    let reply = feeds
        .page(FeedRequest { page: 1, size: 10 })
        .await
        .expect("лента")
        .into_inner();
    reply
        .posts
        .into_iter()
        .map(|post| (post.id, post.author_name, post.likes))
        .collect()
}

#[tokio::test]
async fn a_restart_rebuilds_the_same_feed_from_the_stream() {
    let topics = topics();
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса запущен?");
    let spec = StreamSpec::new(&topics.stream(), topics.stream_subjects());
    ensure_stream(&bus, &spec).await.expect("stream");
    for event in world() {
        let words = topics.event(&event);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let subject = Subject::new(&words).expect("тема");
        publish_event(&bus, &subject, &event.event_id, &event, &Meta::fresh())
            .await
            .expect("событие в stream");
    }
    let expected = vec![
        (3, String::from("alice"), 0),
        (2, String::from("carol"), 0),
        (1, String::from("alice"), 2),
    ];
    let mut pages = Vec::new();
    for _ in 0..2 {
        let (addr, stop, task) = start(&topics).await;
        let began = Instant::now();
        let page = loop {
            let page = page_of(&addr, 2).await;
            if page == expected || began.elapsed() > WAIT {
                break page;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        pages.push(page);
        stop.cancel();
        task.await
            .expect("задача")
            .expect("feed-service остановился");
    }
    assert_eq!(
        pages,
        [expected.clone(), expected],
        "пустой feed-service при каждом старте прочитал stream с начала и пришёл к той же ленте"
    );
    let _ = async_nats::jetstream::new(async_nats::connect(url()).await.expect("подключение"))
        .delete_stream(topics.stream())
        .await;
}
