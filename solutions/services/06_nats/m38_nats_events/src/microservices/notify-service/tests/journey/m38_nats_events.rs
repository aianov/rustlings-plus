use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use contracts::{
    Event, EventBody, EventKind, MarkedView, NoticeAsk, NoticeKind, NoticeView, NoticesView,
    NotifyMethod, PostId, Topics, UnreadView, UserId,
};
use grpc_kit::Stamp;
use nats_engine::{Bus, BusConfig, BusError, DEFAULT_NATS_URL, Envelope, Meta, Pattern, Subject};
use notify_service::controller;
use notify_service::repository::BoxFuture;
use notify_service::{
    Audience, AudienceError, Config, Courier, Decoded, Incoming, MemoryNotices, Notice,
    NotifyService, Stats, decode,
};
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{CreateProfileRequest, PairRequest};
use service_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tonic::transport::Channel;

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
        "n{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса запущен?")
}

async fn until(what: &str, check: impl Fn() -> bool) {
    let began = Instant::now();
    while !check() {
        assert!(began.elapsed() < WAIT, "не дождались: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn subject(words: &[String]) -> Subject {
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    Subject::new(&words).expect("тема")
}

async fn publish(bus: &Bus, topics: &Topics, event: &Event, request_id: &str) {
    let meta = Meta::new(request_id, request_id).expect("meta");
    bus.publish(&subject(&topics.event(event)), &event.body, &meta)
        .await
        .expect("событие ушло");
}

fn envelope(raw: &str, actor: u64, post: Option<u64>) -> Envelope<EventBody> {
    Envelope {
        subject: Subject::parse(raw).expect("тема"),
        meta: Some(Meta::new("req-1", "trace-1").expect("meta")),
        body: EventBody {
            actor: UserId(actor),
            post: post.map(PostId),
        },
    }
}

struct Readers(Result<Vec<UserId>, AudienceError>);

impl Audience for Readers {
    fn readers(&self, _: UserId, _: &str) -> BoxFuture<'_, Result<Vec<UserId>, AudienceError>> {
        let readers = self.0.clone();
        Box::pin(async move { readers })
    }
}

#[derive(Default)]
struct Postbox {
    delivered: Mutex<Vec<Notice>>,
    pause: Duration,
}

impl Postbox {
    fn slow(pause: Duration) -> Postbox {
        Postbox {
            pause,
            ..Postbox::default()
        }
    }

    fn take(&self) -> Vec<Notice> {
        std::mem::take(
            &mut *self
                .delivered
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

impl Courier for Postbox {
    fn deliver(&self, notice: Notice, _: Option<String>) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            tokio::time::sleep(self.pause).await;
            self.delivered
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(notice);
        })
    }
}

fn notify(readers: Vec<UserId>, postbox: Arc<Postbox>) -> Arc<NotifyService> {
    Arc::new(NotifyService::new(
        Arc::new(MemoryNotices::new()),
        Arc::new(Readers(Ok(readers))),
        postbox,
    ))
}

fn liked(actor: u64, owner: u64, post: u64) -> Incoming {
    Incoming {
        kind: EventKind::PostLiked,
        owner: UserId(owner),
        actor: UserId(actor),
        post: Some(PostId(post)),
        request_id: Some(String::from("req-1")),
    }
}

fn config(topics: &Topics, workers: usize, queue: usize) -> Config {
    Config {
        topics: topics.clone(),
        workers,
        queue,
        ..Config::default()
    }
}

#[test]
fn an_event_decodes_into_who_gets_notified() {
    let topics = Topics::new("social");
    let notify = |kind: EventKind, owner: u64, actor: u64, post: Option<u64>| {
        Decoded::Notify(Incoming {
            kind,
            owner: UserId(owner),
            actor: UserId(actor),
            post: post.map(PostId),
            request_id: Some(String::from("req-1")),
        })
    };
    let cases = [
        (
            envelope("social.events.user.followed.u_2", 1, Some(9)),
            notify(EventKind::UserFollowed, 2, 1, None),
            "подписка: пост в теле лишний и отбрасывается",
        ),
        (
            envelope("social.events.post.liked.u_1", 2, Some(7)),
            notify(EventKind::PostLiked, 1, 2, Some(7)),
            "лайк автору поста",
        ),
        (
            envelope("social.events.post.created.u_1", 1, Some(7)),
            notify(EventKind::PostCreated, 1, 1, Some(7)),
            "новый пост: актор и владелец это автор, веер решает сервис",
        ),
        (
            envelope("social.events.post.liked.u_1", 1, Some(7)),
            Decoded::Ignore,
            "свой лайк своего поста не уведомление",
        ),
        (
            envelope("social.events.user.followed.u_1", 2, None),
            notify(EventKind::UserFollowed, 1, 2, None),
            "подписка без поста годится",
        ),
        (
            envelope("other.events.post.liked.u_1", 2, Some(7)),
            Decoded::Ignore,
            "чужой корень",
        ),
        (
            envelope("social.events.user.renamed.u_1", 2, None),
            Decoded::Ignore,
            "незнакомое событие это не ошибка: его просто не ждут",
        ),
    ];
    for (envelope, expected, why) in cases {
        assert_eq!(decode(&topics, &envelope), expected, "{why}");
    }
    for (raw, actor, post, why) in [
        ("social.events.post.liked.u_1", 2, None, "лайк без поста"),
        (
            "social.events.post.commented.u_1",
            2,
            None,
            "комментарий без поста",
        ),
        (
            "social.events.post.created.u_1",
            2,
            Some(7),
            "пост создал не тот, кого называет тема",
        ),
    ] {
        assert!(
            matches!(
                decode(&topics, &envelope(raw, actor, post)),
                Decoded::Reject(_)
            ),
            "{why} это мусор, а не уведомление"
        );
    }
}

#[tokio::test]
async fn an_unread_twin_merges_and_a_read_one_does_not_block() {
    let postbox = Arc::new(Postbox::default());
    let service = notify(Vec::new(), Arc::clone(&postbox));
    let first = service.handle(liked(2, 1, 7)).await.expect("лайк");
    let twin = service.handle(liked(2, 1, 7)).await.expect("тот же лайк");
    assert_eq!(
        (first.stored.len(), twin.stored.len(), twin.duplicates),
        (1, 0, 1),
        "одинаковое непрочитанное уведомление не множится"
    );
    let other = service.handle(liked(3, 1, 7)).await.expect("лайк другого");
    assert_eq!(other.stored.len(), 1, "другой актор это другое уведомление");
    assert_eq!(service.unread(UserId(1)).await, 2);
    assert_eq!(service.mark_read(UserId(1)).await, 2);
    assert_eq!(service.unread(UserId(1)).await, 0);
    let after = service.handle(liked(2, 1, 7)).await.expect("снова лайк");
    assert_eq!(
        after.stored.len(),
        1,
        "прочитанное не мешает новому такому же"
    );
    let listed: Vec<(u64, bool)> = service
        .list(UserId(1), 10)
        .await
        .iter()
        .map(|notice| (notice.id, notice.read))
        .collect();
    assert_eq!(
        listed,
        [(3, false), (2, true), (1, true)],
        "список от новых к старым"
    );
    assert_eq!(service.list(UserId(1), 1).await.len(), 1, "limit режет");
    assert_eq!(
        postbox.take().len(),
        3,
        "каждое сохранённое уведомление ушло дальше, дубликат нет"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn racing_workers_store_one_twin() {
    let service = notify(Vec::new(), Arc::new(Postbox::default()));
    let mut racers = Vec::new();
    for _ in 0..32 {
        let service = Arc::clone(&service);
        racers.push(tokio::spawn(
            async move { service.handle(liked(2, 1, 7)).await },
        ));
    }
    let mut stored = 0;
    for racer in racers {
        stored += racer
            .await
            .expect("задача не упала")
            .expect("лайк")
            .stored
            .len();
    }
    assert_eq!(
        stored, 1,
        "проверка «уже есть» и вставка идут под одним замком хранилища"
    );
}

#[tokio::test]
async fn a_new_post_reaches_the_audience_but_not_the_author() {
    let postbox = Arc::new(Postbox::default());
    let service = notify(vec![UserId(1), UserId(2), UserId(3)], Arc::clone(&postbox));
    let created = Incoming {
        kind: EventKind::PostCreated,
        owner: UserId(1),
        actor: UserId(1),
        post: Some(PostId(7)),
        request_id: None,
    };
    let handled = service.handle(created.clone()).await.expect("веер");
    let fanned: Vec<(UserId, NoticeKind, UserId)> = handled
        .stored
        .iter()
        .map(|notice| (notice.to, notice.kind, notice.actor))
        .collect();
    assert_eq!(
        fanned,
        [
            (UserId(2), NoticeKind::NewPost, UserId(1)),
            (UserId(3), NoticeKind::NewPost, UserId(1))
        ],
        "уведомление каждому читателю из audience, автору нет"
    );
    assert_eq!(postbox.take().len(), 2);
    let broken = NotifyService::new(
        Arc::new(MemoryNotices::new()),
        Arc::new(Readers(Err(AudienceError::Unavailable))),
        Arc::new(Postbox::default()),
    );
    assert_eq!(
        broken.handle(created).await,
        Err(AudienceError::Unavailable),
        "без читателей веер не делает вид, что всё хорошо"
    );
}

#[tokio::test]
async fn requests_answer_the_bell() {
    let (bus, topics) = (bus().await, topics());
    let service = notify(Vec::new(), Arc::new(Postbox::default()));
    let stats = Arc::new(Stats::default());
    let running = controller::start(&bus, &config(&topics, 2, 64), service, Arc::clone(&stats))
        .await
        .expect("контроллер подписался");
    for actor in [2, 3] {
        publish(
            &bus,
            &topics,
            &Event::post_liked(UserId(actor), UserId(1), PostId(7)),
            "req-1",
        )
        .await;
    }
    publish(
        &bus,
        &topics,
        &Event::followed(UserId(4), UserId(1)),
        "req-2",
    )
    .await;
    until("три уведомления сохранены", || {
        stats.snapshot().stored == 3
    })
    .await;
    let meta = Meta::fresh();
    let ask = |limit| NoticeAsk {
        user: UserId(1),
        limit,
    };
    let unread: UnreadView = bus
        .request(
            &subject(&topics.notify(NotifyMethod::Unread)),
            &ask(None),
            &meta,
            WAIT,
        )
        .await
        .expect("unread");
    assert_eq!(unread.unread, 3);
    let list: NoticesView = bus
        .request(
            &subject(&topics.notify(NotifyMethod::List)),
            &ask(Some(2)),
            &meta,
            WAIT,
        )
        .await
        .expect("list");
    assert_eq!(
        (list.notices.len(), list.unread),
        (2, 3),
        "в списке не больше limit, а счётчик про всех"
    );
    let refused = bus
        .request::<NoticeAsk, NoticesView>(
            &subject(&topics.notify(NotifyMethod::List)),
            &ask(Some(0)),
            &meta,
            WAIT,
        )
        .await;
    assert!(
        matches!(&refused, Err(BusError::Remote(text)) if text.contains("limit")),
        "негодный limit это ответ-ошибка, а не тишина: {refused:?}"
    );
    let marked: MarkedView = bus
        .request(
            &subject(&topics.notify(NotifyMethod::Read)),
            &ask(None),
            &meta,
            WAIT,
        )
        .await
        .expect("read");
    assert_eq!(marked.marked, 3);
    let after: UnreadView = bus
        .request(
            &subject(&topics.notify(NotifyMethod::Unread)),
            &ask(None),
            &meta,
            WAIT,
        )
        .await
        .expect("unread");
    assert_eq!(after.unread, 0);
    running.stop().await;
    assert!(stats.snapshot().balanced(), "{:?}", stats.snapshot());
}

#[tokio::test]
async fn broken_and_foreign_events_are_counted_not_lost() {
    let (bus, topics) = (bus().await, topics());
    let raw = async_nats::connect(url()).await.expect("сырое подключение");
    let stats = Arc::new(Stats::default());
    let service = notify(Vec::new(), Arc::new(Postbox::default()));
    let running = controller::start(&bus, &config(&topics, 1, 8), service, Arc::clone(&stats))
        .await
        .expect("контроллер");
    let root = topics.root();
    raw.publish(format!("{root}.events.post.liked.u_1"), "{не json".into())
        .await
        .expect("мусор");
    raw.publish(
        format!("{root}.events.user.renamed.u_1"),
        r#"{"actor": "u_2"}"#.into(),
    )
    .await
    .expect("незнакомое");
    raw.publish(
        format!("{root}.events.post.liked.u_1"),
        r#"{"actor": "u_2"}"#.into(),
    )
    .await
    .expect("без поста");
    publish(
        &bus,
        &topics,
        &Event::post_liked(UserId(1), UserId(1), PostId(7)),
        "req-1",
    )
    .await;
    publish(
        &bus,
        &topics,
        &Event::post_liked(UserId(2), UserId(1), PostId(7)),
        "req-2",
    )
    .await;
    until("пять событий получены", || {
        stats.snapshot().received == 5
    })
    .await;
    until("годное сохранено", || {
        stats.snapshot().stored == 1
    })
    .await;
    let seen = stats.snapshot();
    assert_eq!(
        (seen.rejected, seen.ignored, seen.queued, seen.dropped),
        (2, 2, 1, 0),
        "сломанный JSON и лайк без поста отвергнуты, незнакомое и свой лайк пропущены: {seen:?}"
    );
    assert!(
        seen.balanced(),
        "received = ignored + rejected + queued + dropped: {seen:?}"
    );
    running.stop().await;
}

#[tokio::test]
async fn a_full_queue_drops_and_counts_instead_of_waiting() {
    let (bus, topics) = (bus().await, topics());
    let stats = Arc::new(Stats::default());
    let postbox = Arc::new(Postbox::slow(Duration::from_millis(200)));
    let service = notify(Vec::new(), postbox);
    let running = controller::start(&bus, &config(&topics, 1, 1), service, Arc::clone(&stats))
        .await
        .expect("контроллер");
    for actor in 2..32 {
        publish(
            &bus,
            &topics,
            &Event::post_liked(UserId(actor), UserId(1), PostId(7)),
            "req-1",
        )
        .await;
    }
    until("все 30 событий приняты", || {
        stats.snapshot().received == 30
    })
    .await;
    let seen = stats.snapshot();
    assert!(
        seen.dropped > 0 && seen.queued >= 1,
        "один работник занят, очередь на одно место: лишнее выброшено и посчитано: {seen:?}"
    );
    assert!(seen.balanced(), "{seen:?}");
    running.stop().await;
    assert_eq!(
        stats.snapshot().stored,
        stats.snapshot().queued,
        "остановка дождалась того, что уже было в очереди"
    );
}

#[tokio::test]
async fn a_group_shares_the_stream_without_doubles() {
    let (bus, topics) = (bus().await, topics());
    let (left, right) = (Arc::new(Stats::default()), Arc::new(Stats::default()));
    let first = controller::start(
        &bus,
        &config(&topics, 2, 128),
        notify(Vec::new(), Arc::new(Postbox::default())),
        Arc::clone(&left),
    )
    .await
    .expect("первый экземпляр");
    let other = Bus::connect(BusConfig::new(url()))
        .await
        .expect("второе подключение");
    let second = controller::start(
        &other,
        &config(&topics, 2, 128),
        notify(Vec::new(), Arc::new(Postbox::default())),
        Arc::clone(&right),
    )
    .await
    .expect("второй экземпляр");
    for actor in 2..42 {
        publish(
            &bus,
            &topics,
            &Event::post_liked(UserId(actor), UserId(1), PostId(7)),
            "req-1",
        )
        .await;
    }
    until(
        "40 событий разобраны группой",
        || left.snapshot().stored + right.snapshot().stored == 40,
    )
    .await;
    let (l, r) = (left.snapshot(), right.snapshot());
    assert_eq!(
        l.received + r.received,
        40,
        "каждое событие получил ровно один экземпляр группы: {l:?} {r:?}"
    );
    assert!(
        l.received > 0 && r.received > 0,
        "работа поделена между экземплярами: {l:?} {r:?}"
    );
    first.stop().await;
    second.stop().await;
}

struct Proxy {
    addr: SocketAddr,
    cut: Arc<AtomicBool>,
    severed: Arc<Notify>,
}

impl Proxy {
    async fn start() -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт прокси");
        let addr = listener.local_addr().expect("адрес прокси");
        let (cut, severed) = (Arc::new(AtomicBool::new(false)), Arc::new(Notify::new()));
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
        Proxy { addr, cut, severed }
    }

    fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        self.severed.notify_waiters();
    }

    fn restore(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }
}

async fn ready_status(admin: SocketAddr) -> u16 {
    let Ok(mut stream) = TcpStream::connect(admin).await else {
        return 0;
    };
    let request = format!("GET /ready HTTP/1.1\r\nHost: {admin}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).await.is_err() {
        return 0;
    }
    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply).await;
    String::from_utf8_lossy(&reply)
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

#[tokio::test]
async fn readiness_follows_the_broker() {
    let proxy = Proxy::start().await;
    let admin = TcpListener::bind("127.0.0.1:0").await.expect("порт admin");
    let admin_addr = admin.local_addr().expect("адрес admin");
    let stop = CancellationToken::new();
    let config = Config {
        nats: BusConfig::new(format!("nats://{}", proxy.addr)),
        topics: topics(),
        ..Config::default()
    };
    let running = notify_service::start(Some(admin), config, stop.clone())
        .await
        .expect("notify-service стартовал");
    assert!(running.ready());
    assert_eq!(ready_status(admin_addr).await, 200, "со связью готов");
    proxy.cut();
    until("готовность упала после обрыва NATS", || !running.ready()).await;
    assert_eq!(
        ready_status(admin_addr).await,
        503,
        "без брокера notify-service честно не готов: событий он не получит"
    );
    proxy.restore();
    until("связь вернулась", || running.ready()).await;
    assert_eq!(ready_status(admin_addr).await, 200);
    stop.cancel();
    running.finished().await.expect("остановился чисто");
}

#[tokio::test]
async fn a_new_post_fans_out_through_user_service() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let users_addr = listener.local_addr().expect("адрес");
    let stop = CancellationToken::new();
    let users_task = tokio::spawn(user_service::run(
        listener,
        user_service::Config::default(),
        stop.clone(),
    ));
    let channel = Channel::from_shared(format!("http://{users_addr}"))
        .expect("адрес")
        .connect()
        .await
        .expect("user-service отвечает");
    let mut ids = Vec::new();
    for name in ["alice", "bob", "carol", "dave"] {
        let created = UsersClient::new(channel.clone())
            .create_profile(CreateProfileRequest {
                username: name.to_string(),
            })
            .await
            .expect("профиль")
            .into_inner();
        ids.push(created.id);
    }
    let (alice, bob, carol) = (UserId(ids[0]), UserId(ids[1]), UserId(ids[2]));
    for reader in [bob, carol] {
        UsersClient::with_interceptor(channel.clone(), Stamp::fresh().user(reader.0))
            .follow(PairRequest { target_id: alice.0 })
            .await
            .expect("подписка");
    }
    UsersClient::with_interceptor(channel.clone(), Stamp::fresh().user(carol.0))
        .mute(PairRequest { target_id: alice.0 })
        .await
        .expect("скрыть автора");
    let topics = topics();
    let config = Config {
        users: users_addr.to_string(),
        nats: BusConfig::new(url()),
        topics: topics.clone(),
        ..Config::default()
    };
    let running = notify_service::start(None, config, stop.child())
        .await
        .expect("notify-service стартовал");
    let bus = bus().await;
    let watch = Pattern::parse(&topics.all_notices().join(".")).expect("шаблон");
    let mut notices = bus
        .subscribe::<NoticeView>(&watch)
        .await
        .expect("подписка на уведомления");
    publish(
        &bus,
        &topics,
        &Event::post_created(alice, PostId(7)),
        "e856e880e9a85e6d",
    )
    .await;
    let got = notices
        .next_within(WAIT)
        .await
        .expect("уведомление пришло")
        .expect("уведомление разобралось");
    assert_eq!(
        got.subject.as_str(),
        format!("{}.notices.{bob}", topics.root()),
        "уведомление уходит в тему адресата"
    );
    assert_eq!(
        (got.body.to, got.body.kind, got.body.actor, got.body.post),
        (bob, NoticeKind::NewPost, alice, Some(PostId(7)))
    );
    assert_eq!(
        got.meta.map(|meta| meta.request_id),
        Some(String::from("e856e880e9a85e6d")),
        "номер запроса события едет дальше в уведомление"
    );
    assert!(
        notices
            .next_within(Duration::from_millis(300))
            .await
            .is_none(),
        "carol скрыла автора, dave не подписан, alice автор: больше уведомлений нет"
    );
    stop.cancel();
    running
        .finished()
        .await
        .expect("notify-service остановился");
    users_task
        .await
        .expect("задача user-service")
        .expect("user-service остановился");
}
