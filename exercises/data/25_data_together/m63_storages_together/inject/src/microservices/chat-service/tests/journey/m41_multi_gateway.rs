use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chat_service::repository::Stores;
use chat_service::{Config, run};
use contracts::{DomainEvent, Fact, Topics, TypingNote, UserId};
use futures::StreamExt;
use grpc_kit::Stamp;
use nats_engine::js::live;
use nats_engine::{Bus, BusConfig, DEFAULT_NATS_URL, Subject};
use proto::social::v1::chat_client::ChatClient;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{CreateProfileRequest, SendRequest, TypingRequest};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
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
        "c{}_{nanos:x}_{}",
        std::process::id() % 100_000,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn js() -> async_nats::jetstream::Context {
    async_nats::jetstream::new(
        async_nats::connect(url())
            .await
            .expect("NATS курса запущен?"),
    )
}

async fn stored(topics: &Topics) -> u64 {
    let mut stream = js()
        .await
        .get_stream(topics.chat_stream())
        .await
        .expect("chat-service завёл stream чата");
    stream.info().await.expect("сведения").state.messages
}

fn lazy(addr: &str) -> Channel {
    Channel::from_shared(format!("http://{addr}"))
        .expect("адрес")
        .connect_lazy()
}

struct Stack {
    chat: Channel,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<Result<(), String>>>,
}

impl Stack {
    async fn start(topics: &Topics, stores: &Stores) -> Stack {
        let stop = CancellationToken::new();
        let users_listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
        let users = users_listener.local_addr().expect("адрес").to_string();
        let chat_listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
        let chat = chat_listener.local_addr().expect("адрес").to_string();
        let config = Config {
            users: users.clone(),
            stores: Some(stores.clone()),
            nats: Some(BusConfig::new(url()).named("chat-service")),
            topics: topics.clone(),
            ..Config::default()
        };
        let users_task = tokio::spawn({
            let stop = stop.clone();
            async move {
                user_service::run(users_listener, user_service::Config::default(), stop)
                    .await
                    .map_err(|e| format!("{e:?}"))
            }
        });
        let chat_task = tokio::spawn({
            let stop = stop.clone();
            async move {
                run(chat_listener, config, stop)
                    .await
                    .map_err(|e| format!("{e:?}"))
            }
        });
        for name in ["alice", "bob"] {
            let ask = CreateProfileRequest {
                username: name.to_string(),
            };
            UsersClient::new(lazy(&users))
                .create_profile(ask)
                .await
                .unwrap_or_else(|e| panic!("профиль {name}: {e:?}"));
        }
        Stack {
            chat: lazy(&chat),
            stop,
            tasks: vec![chat_task, users_task],
        }
    }

    fn by(&self, user: u64) -> ChatClient<InterceptedService<Channel, Stamp>> {
        ChatClient::with_interceptor(self.chat.clone(), Stamp::fresh().user(user))
    }

    async fn stop(self) {
        self.stop.cancel();
        for task in self.tasks {
            let finished = task.await.expect("задача не падает");
            assert!(
                finished.is_ok(),
                "сервис останавливается без ошибки: {finished:?}"
            );
        }
    }
}

#[tokio::test]
async fn a_message_and_its_event_are_one_line_and_reach_the_live_subject() {
    let topics = topics();
    let path = Stores::memory();
    let bus = Bus::connect(BusConfig::new(url()))
        .await
        .expect("NATS курса");
    let words = topics.live(UserId(2));
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let stack = Stack::start(&topics, &path).await;
    let mut inbox = live::<DomainEvent>(&bus, &Subject::new(&words).expect("тема"))
        .await
        .expect("живая тема bob");
    let ask = SendRequest {
        ttl_seconds: 0,
        peer_id: 2,
        text: "привет, bob".to_string(),
    };
    stack.by(1).send(ask).await.expect("отправка");
    let echoed = timeout(WAIT, inbox.next())
        .await
        .expect("сообщение дошло до живой темы получателя")
        .expect("подписка жива");
    assert!(echoed.sequence > 0, "у сообщения номер stream");
    let event = echoed.item.expect("конверт разобран").body;
    assert_eq!(
        (
            event.kind.as_str(),
            event.aggregate_id.as_str(),
            event.version
        ),
        ("chat.sent", "u_2", 1),
        "событие chat.sent, агрегат это получатель, версия по получателю"
    );
    match event.fact() {
        Ok(Some(Fact::MessageSent(sent))) => assert_eq!(
            (sent.from_id, sent.to_id, sent.text.as_str()),
            (UserId(1), UserId(2), "привет, bob")
        ),
        other => panic!("ждали chat.sent, а разобралось {other:?}"),
    }
    assert_eq!(stored(&topics).await, 1, "сообщение лежит в stream чата");
    stack.stop().await;
    inbox.stop().await;
    assert_eq!(path.outbox.waiting().await, 0, "релей отметил отправленное");
    let _ = &event;
    let _ = js().await.delete_stream(topics.chat_stream()).await;
}

#[tokio::test]
async fn typing_is_paced_checked_and_never_stored() {
    let topics = topics();
    let path = Stores::memory();
    let client = async_nats::connect(url()).await.expect("NATS курса");
    let mut typing = client
        .subscribe(topics.typing(UserId(2)).join("."))
        .await
        .expect("подписка на «печатает» bob");
    client.flush().await.expect("flush");
    let stack = Stack::start(&topics, &path).await;
    let ask = TypingRequest { peer_id: 2 };
    let first = stack.by(1).typing(ask).await.expect("печатает");
    assert!(first.into_inner().sent, "первое «печатает» уходит");
    let note = timeout(WAIT, typing.next())
        .await
        .expect("«печатает» пришло")
        .expect("подписка жива");
    let note: TypingNote = serde_json::from_slice(&note.payload).expect("тело");
    assert_eq!((note.from, note.to), (UserId(1), UserId(2)));
    let second = stack.by(1).typing(ask).await.expect("печатает снова");
    assert!(
        !second.into_inner().sent,
        "пара (alice, bob) уже писала меньше 3 секунд назад"
    );
    let other = stack
        .by(2)
        .typing(TypingRequest { peer_id: 1 })
        .await
        .expect("bob печатает alice");
    assert!(other.into_inner().sent, "у обратной пары свой счёт");
    assert!(
        timeout(Duration::from_millis(300), typing.next())
            .await
            .is_err(),
        "второе «печатает» в тему bob не ушло"
    );
    let own = stack
        .by(1)
        .typing(TypingRequest { peer_id: 1 })
        .await
        .expect_err("себе");
    assert_eq!(own.code(), Code::FailedPrecondition);
    let ghost = stack
        .by(1)
        .typing(TypingRequest { peer_id: 99 })
        .await
        .expect_err("нет такого");
    assert_eq!(ghost.code(), Code::NotFound);
    assert_eq!(
        stored(&topics).await,
        0,
        "«печатает» в stream чата не пишется"
    );
    stack.stop().await;
    let _ = js().await.delete_stream(topics.chat_stream()).await;
}
