use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use contracts::{DomainEvent, Fact, NoticeKind, NoticeView, PostId, PostLiked, Topics, UserId};
use nats_engine::js::{StreamSpec, ensure_stream, publish_event};
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, Meta, Pattern, Subject};
use notify_service::repository::BoxFuture;
use notify_service::{
    Audience, AudienceError, Config, Incoming, MemoryNotices, NotifyService, Quiet, Trigger,
};
use service_runtime::CancellationToken;

const WAIT: Duration = Duration::from_secs(10);

struct Nobody;

impl Audience for Nobody {
    fn readers(&self, _: UserId, _: &str) -> BoxFuture<'_, Result<Vec<UserId>, AudienceError>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

fn liked(owner: u64, actor: u64) -> Incoming {
    Incoming {
        kind: Trigger::Liked,
        owner: UserId(owner),
        actor: UserId(actor),
        post: Some(PostId(7)),
        request_id: None,
        event: None,
    }
}

#[tokio::test]
async fn a_digest_counts_unread_and_is_not_repeated() {
    let service = NotifyService::new(
        Arc::new(MemoryNotices::new()),
        Arc::new(Nobody),
        Arc::new(Quiet),
    );
    for actor in [2, 3] {
        service.handle(liked(1, actor)).await.expect("лайк");
    }
    service.handle(liked(4, 2)).await.expect("лайк");
    service.mark_read(UserId(4)).await;
    assert_eq!(
        service.digest_targets().await,
        [(UserId(1), 2)],
        "сводка только тем, у кого есть непрочитанное"
    );
    assert!(service.send_digest(UserId(1), 2, "digest.1:1").await);
    assert!(
        !service.send_digest(UserId(1), 3, "digest.1:1").await,
        "пока сводка не прочитана, вторая не появляется: повтор задачи безвреден"
    );
    let bell = service.list(UserId(1), 10).await;
    assert_eq!(
        (bell[0].kind, bell[0].count),
        (NoticeKind::Digest, Some(2)),
        "сводка лежит в колокольчике с числом непрочитанных"
    );
    service.mark_read(UserId(1)).await;
    assert!(
        service.send_digest(UserId(1), 1, "digest.2:1").await,
        "прочитанная сводка не мешает следующей"
    );
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
        "z{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn drop_streams(topics: &Topics) {
    if let Ok(client) = async_nats::connect(url()).await {
        let js = async_nats::jetstream::new(client);
        let names = [
            topics.stream(),
            topics.jobs_stream(),
            topics.jobs_dead_stream(),
            format!("KV_{}", topics.checkpoints()),
            format!("KV_{}_DIGEST", topics.leases()),
        ];
        for name in names {
            let _ = js.delete_stream(name).await;
        }
    }
}

#[tokio::test]
async fn the_digest_job_runs_once_per_period_for_the_cluster() {
    let topics = topics();
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса");
    let spec = StreamSpec::new(&topics.stream(), topics.stream_subjects());
    ensure_stream(&bus, &spec).await.expect("stream событий");
    let watch = Pattern::parse(&topics.all_notices().join(".")).expect("шаблон");
    let mut notices = bus
        .subscribe::<NoticeView>(&watch)
        .await
        .expect("подписка на уведомления");
    let config = Config {
        nats: BusConfig::new(url()),
        topics: topics.clone(),
        digest_every: Duration::from_millis(400),
        ..Config::default()
    };
    for actor in [2, 3] {
        let fact = Fact::PostLiked(PostLiked {
            post_id: PostId(7),
            author_id: UserId(1),
            user_id: UserId(actor),
        });
        let id = format!("like-{actor}");
        let event = DomainEvent::new(&id, &fact, 1, 1);
        let words = topics.event(&event);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let subject = Subject::new(&words).expect("тема");
        publish_event(&bus, &subject, &id, &event, &Meta::fresh())
            .await
            .expect("событие");
    }
    let stop = CancellationToken::new();
    let mut instances = Vec::new();
    for _ in 0..2 {
        let started = notify_service::start(None, config.clone(), stop.clone())
            .await
            .expect("notify-service");
        instances.push(started);
    }
    let mut digests = Vec::new();
    let began = Instant::now();
    while began.elapsed() < Duration::from_millis(2500) {
        if let Some(Ok(envelope)) = notices.next_within(Duration::from_millis(100)).await {
            if envelope.body.kind == NoticeKind::Digest {
                digests.push(envelope.body);
            }
        }
    }
    assert!(began.elapsed() < WAIT);
    assert!(
        !digests.is_empty(),
        "сводка пришла уведомлением: задача поставлена лидером и выполнена воркером"
    );
    assert!(
        digests
            .iter()
            .all(|digest| digest.to == UserId(1) && digest.count == Some(2)),
        "сводка для того, у кого два непрочитанных: {digests:?}"
    );
    assert_eq!(
        digests.len(),
        1,
        "два экземпляра и несколько периодов, а непрочитанная сводка одна: {digests:?}"
    );
    stop.cancel();
    for instance in instances {
        instance.finished().await.expect("остановился чисто");
    }
    drop_streams(&topics).await;
}
