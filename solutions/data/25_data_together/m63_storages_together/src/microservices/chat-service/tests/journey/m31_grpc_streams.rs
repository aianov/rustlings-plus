use chat_service::repository::Stores;
use chat_service::{Config, run};
use grpc_kit::Stamp;
use proto::domain_code;
use proto::social::v1::chat_client::ChatClient;
use proto::social::v1::chat_server::SERVICE_NAME;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{CreateProfileRequest, HistoryRequest, SendRequest};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus;
use tonic_health::pb::health_client::HealthClient;

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    (listener, addr)
}

fn lazy(addr: &str) -> Channel {
    Channel::from_shared(format!("http://{addr}"))
        .expect("адрес")
        .connect_lazy()
}

struct Stack {
    users: Channel,
    chat: Channel,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<Result<(), String>>>,
}

impl Stack {
    async fn start(stores: Option<&Stores>) -> Stack {
        let stop = CancellationToken::new();
        let (users_listener, users) = listener().await;
        let (chat_listener, chat) = listener().await;
        let config = Config {
            users: users.clone(),
            stores: Some(stores.cloned().unwrap_or_else(Stores::memory)),
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
        let stack = Stack {
            users: lazy(&users),
            chat: lazy(&chat),
            stop,
            tasks: vec![chat_task, users_task],
        };
        for name in ["alice", "bob", "carol"] {
            let ask = CreateProfileRequest {
                username: name.to_string(),
            };
            UsersClient::new(stack.users.clone())
                .create_profile(ask)
                .await
                .unwrap_or_else(|e| panic!("профиль {name}: {e:?}"));
        }
        stack
    }

    fn by(&self, user: u64) -> ChatClient<InterceptedService<Channel, Stamp>> {
        ChatClient::with_interceptor(self.chat.clone(), Stamp::fresh().user(user))
    }

    async fn send(&self, from: u64, to: u64, text: &str) -> Result<(), Code> {
        let ask = SendRequest {
            ttl_seconds: 0,
            peer_id: to,
            text: text.to_string(),
        };
        self.by(from)
            .send(ask)
            .await
            .map(drop)
            .map_err(|s| s.code())
    }

    async fn history(&self, user: u64, peer: u64) -> Vec<(u64, String)> {
        let ask = HistoryRequest {
            cursor: String::new(),
            peer_id: peer,
            limit: 0,
        };
        let reply = self.by(user).history(ask).await.expect("история");
        let messages = reply.into_inner().messages;
        messages.into_iter().map(|m| (m.from_id, m.text)).collect()
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
async fn messages_are_checked_stored_and_listed() {
    let stack = Stack::start(None).await;
    stack
        .send(1, 2, "привет, bob")
        .await
        .expect("alice пишет bob");
    stack
        .send(2, 1, "привет, alice")
        .await
        .expect("bob отвечает");
    assert_eq!(
        stack.history(2, 1).await,
        [
            (1, "привет, bob".to_string()),
            (2, "привет, alice".to_string())
        ],
        "диалог один на двоих, старые первыми, отправитель из x-user-id"
    );
    assert_eq!(
        stack.send(1, 99, "кто ты").await,
        Err(Code::NotFound),
        "собеседника нет в user-service"
    );
    assert_eq!(
        stack.send(1, 1, "сам себе").await,
        Err(Code::FailedPrecondition)
    );
    assert_eq!(
        stack.send(1, 2, "   ").await,
        Err(Code::InvalidArgument),
        "пустой текст это ошибка поля text, а не правило переписки"
    );
    let anonymous = ChatClient::new(stack.chat.clone())
        .send(SendRequest {
            ttl_seconds: 0,
            peer_id: 2,
            text: "чьё".to_string(),
        })
        .await
        .expect_err("без x-user-id");
    assert_eq!(
        (anonymous.code(), domain_code(&anonymous)),
        (Code::Unauthenticated, Some("unauthorized"))
    );
    let mut health = HealthClient::new(stack.chat.clone());
    let ask = HealthCheckRequest {
        service: SERVICE_NAME.to_string(),
    };
    let status = health
        .check(ask)
        .await
        .expect("здоровье отвечает")
        .into_inner();
    assert_eq!(
        status.status(),
        ServingStatus::Serving,
        "chat-service готов и говорит об этом"
    );
    stack.stop().await;
}

#[tokio::test]
async fn messages_survive_restart_on_the_same_store() {
    let stores = Stores::memory();
    let stack = Stack::start(Some(&stores)).await;
    stack.send(1, 2, "до перезапуска").await.expect("сообщение");
    stack.stop().await;
    let stack = Stack::start(Some(&stores)).await;
    assert_eq!(
        stack.history(1, 2).await,
        [(1, "до перезапуска".to_string())],
        "с m63 переписку хранит хранилище, а не журнал процесса"
    );
    stack.send(2, 1, "после").await.expect("новое сообщение");
    stack.stop().await;
    assert_eq!(
        stores.outbox.waiting().await,
        2,
        "оба сообщения ждут релея в outbox"
    );
}
