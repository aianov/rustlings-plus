use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use nats_engine::{
    Bus, BusConfig, BusError, DEFAULT_NATS_URL, Envelope, Meta, Pattern, REQUEST_ID, Seg, Subject,
    SubjectError, TRACE_ID, good_id,
};
use serde::{Deserialize, Serialize};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(3);

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
        "e{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

async fn bus() -> Bus {
    Bus::connect(BusConfig::new(url()).named("m38-test"))
        .await
        .expect("шина подключилась к NATS курса: он запущен?")
}

fn subject(root: &str, tail: &[&str]) -> Subject {
    let mut words = vec![root];
    words.extend_from_slice(tail);
    Subject::new(&words).expect("тема")
}

fn pattern(root: &str, tail: &[Seg<'_>]) -> Pattern {
    let mut segments = vec![Seg::Word(root)];
    segments.extend_from_slice(tail);
    Pattern::build(&segments).expect("шаблон")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Liked {
    post: u64,
    by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Ask {
    user: String,
}

struct Proxy {
    addr: SocketAddr,
    cut: Arc<AtomicBool>,
    severed: Arc<Notify>,
}

impl Proxy {
    async fn start(upstream: String) -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт прокси");
        let addr = listener.local_addr().expect("адрес прокси");
        let (cut, severed) = (Arc::new(AtomicBool::new(false)), Arc::new(Notify::new()));
        let target = upstream.trim_start_matches("nats://").to_string();
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

    fn url(&self) -> String {
        format!("nats://{}", self.addr)
    }

    fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        self.severed.notify_waiters();
    }

    fn restore(&self) {
        self.cut.store(false, Ordering::SeqCst);
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    let began = Instant::now();
    while !check() {
        assert!(began.elapsed() < WAIT, "не дождались: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn subjects_take_only_checked_words() {
    assert_eq!(
        Subject::new(&["social", "events", "user", "followed", "u_2"])
            .expect("тема")
            .as_str(),
        "social.events.user.followed.u_2"
    );
    for (word, error) in [
        ("*", SubjectError::BadChar('*')),
        (">", SubjectError::BadChar('>')),
        ("a.b", SubjectError::BadChar('.')),
        ("a b", SubjectError::BadChar(' ')),
        ("a\r\nPUB x 1", SubjectError::BadChar('\r')),
        ("", SubjectError::Empty),
    ] {
        assert_eq!(
            Subject::new(&["social", word]),
            Err(error),
            "{word:?} не слово темы: подставить чужой ввод в тему нельзя"
        );
    }
    assert_eq!(
        Pattern::build(&[Seg::Word("social"), Seg::Word("*")]),
        Err(SubjectError::BadChar('*')),
        "звёздочка внутри слова это ошибка, подстановку даёт только Seg::Any"
    );
    assert_eq!(
        Pattern::build(&[Seg::Tail, Seg::Word("x")]),
        Err(SubjectError::BadWildcard)
    );
    let events = Pattern::parse("social.events.>").expect("шаблон из конфига");
    let one = Subject::parse("social.events.post.liked.u_1").expect("тема");
    assert!(events.matches(&one));
    assert!(!events.matches(&Subject::parse("social.notices.u_1").expect("тема")));
}

#[test]
fn meta_follows_the_request_or_starts_a_fresh_one() {
    let kept = Meta::follow("e856e880e9a85e6d", Some("trace-7"));
    assert_eq!(
        (kept.request_id.as_str(), kept.trace_id.as_str()),
        ("e856e880e9a85e6d", "trace-7"),
        "годные номера переезжают в заголовки как есть"
    );
    let same = Meta::follow("e856e880e9a85e6d", None);
    assert_eq!(
        same.trace_id, same.request_id,
        "без трассы её начинает сам запрос"
    );
    let fresh = Meta::follow("bad id\r\n", Some("ok"));
    assert!(
        fresh.request_id.starts_with("req-") && good_id(&fresh.request_id),
        "негодный номер заменяется свежим, а не уходит в заголовок: {fresh:?}"
    );
    assert_eq!(fresh.trace_id, "ok");
    assert!(Meta::new("a", "b c").is_err(), "пробел в идентификаторе");
    assert!(!good_id(&"x".repeat(65)), "не длиннее 64");
}

#[tokio::test]
async fn a_typed_event_carries_exactly_two_ids() {
    let (bus, root) = (bus().await, root());
    let raw = async_nats::connect(url()).await.expect("сырое подключение");
    let mut wire = raw
        .subscribe(format!("{root}.post.liked.alice"))
        .await
        .expect("сырая подписка");
    raw.flush().await.expect("подписка дошла");
    let mut typed = bus
        .subscribe::<Liked>(&pattern(&root, &[Seg::Word("post"), Seg::Any, Seg::Any]))
        .await
        .expect("подписка");
    let meta = Meta::new("req-1", "trace-1").expect("meta");
    let body = Liked {
        post: 7,
        by: String::from("bob"),
    };
    bus.publish(&subject(&root, &["post", "liked", "alice"]), &body, &meta)
        .await
        .expect("публикация");
    let got = typed
        .next_within(WAIT)
        .await
        .expect("событие пришло")
        .expect("событие разобралось");
    assert_eq!(
        got,
        Envelope {
            subject: subject(&root, &["post", "liked", "alice"]),
            meta: Some(meta),
            body
        }
    );
    let message = timeout(WAIT, wire.next())
        .await
        .expect("сырое сообщение пришло")
        .expect("поток жив");
    let headers = message.headers.expect("заголовки есть");
    let mut names: Vec<String> = headers.iter().map(|(name, _)| name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [REQUEST_ID, TRACE_ID],
        "на проводе ровно два заголовка: номер запроса и трасса"
    );
}

#[tokio::test]
async fn a_broken_message_is_one_error_and_the_stream_goes_on() {
    let (bus, root) = (bus().await, root());
    let raw = async_nats::connect(url()).await.expect("сырое подключение");
    let mut typed = bus
        .subscribe::<Liked>(&pattern(&root, &[Seg::Tail]))
        .await
        .expect("подписка");
    let body = Liked {
        post: 1,
        by: String::from("bob"),
    };
    let good = serde_json::to_vec(&body).expect("JSON");
    raw.publish(format!("{root}.x"), "не json".into())
        .await
        .expect("мусор");
    raw.publish(format!("{root}.y"), good.into())
        .await
        .expect("годное следом по тому же соединению");
    let broken = typed.next_within(WAIT).await.expect("пришло");
    assert!(matches!(broken, Err(BusError::Decode(_))), "{broken:?}");
    let good = typed.next_within(WAIT).await.expect("пришло");
    assert_eq!(good.map(|envelope| envelope.body), Ok(body));
}

#[tokio::test]
async fn request_outcomes_are_told_apart_and_handlers_are_async() {
    let (bus, root) = (bus().await, root());
    let ask = subject(&root, &["notify", "unread"]);
    let meta = Meta::fresh();
    let nobody = bus
        .request::<Ask, u32>(&ask, &Ask { user: "u_1".into() }, &meta, WAIT)
        .await;
    assert_eq!(
        nobody,
        Err(BusError::NoService),
        "слушателей нет: брокер говорит сразу"
    );
    let server = bus
        .serve(
            &pattern(&root, &[Seg::Word("notify"), Seg::Word("unread")]),
            Some("notify-service"),
            |ask: Envelope<Ask>| async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                match ask.body.user.as_str() {
                    "u_1" => Ok(3_u32),
                    "slow" => {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        Ok(0)
                    }
                    other => Err(format!("нет пользователя {other}")),
                }
            },
        )
        .await
        .expect("ответчик");
    assert_eq!(
        bus.request::<Ask, u32>(&ask, &Ask { user: "u_1".into() }, &meta, WAIT)
            .await,
        Ok(3),
        "асинхронный обработчик отвечает значением"
    );
    assert_eq!(
        bus.request::<Ask, u32>(&ask, &Ask { user: "u_9".into() }, &meta, WAIT)
            .await,
        Err(BusError::Remote(String::from("нет пользователя u_9")))
    );
    assert_eq!(
        bus.request::<Ask, u32>(
            &ask,
            &Ask {
                user: "slow".into()
            },
            &meta,
            Duration::from_millis(100)
        )
        .await,
        Err(BusError::Timeout)
    );
    server.stop().await;
    assert_eq!(
        bus.request::<Ask, u32>(&ask, &Ask { user: "u_1".into() }, &meta, WAIT)
            .await,
        Err(BusError::NoService),
        "остановленный ответчик снят до возврата из stop"
    );
}

#[tokio::test]
async fn without_the_broker_publishing_fails_fast_and_readiness_drops() {
    let proxy = Proxy::start(url()).await;
    let bus = Bus::connect(BusConfig::new(proxy.url()))
        .await
        .expect("через прокси");
    let root = root();
    assert!(bus.ready(), "подключились");
    proxy.cut();
    until("готовность упала после обрыва", || !bus.ready()).await;
    let began = Instant::now();
    let meta = Meta::fresh();
    for n in 0..3000_u64 {
        let body = Liked {
            post: n,
            by: String::from("bob"),
        };
        let sent = bus.publish(&subject(&root, &["lost"]), &body, &meta).await;
        assert_eq!(sent, Err(BusError::Unavailable), "без связи сразу отказ");
    }
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "3000 отказов без ожидания, ничего не копится в скрытом буфере: {:?}",
        began.elapsed()
    );
    assert_eq!(bus.sync().await, Err(BusError::Unavailable));
    proxy.restore();
    until("связь вернулась сама", || bus.ready()).await;
    assert_eq!(bus.sync().await, Ok(()));
}
