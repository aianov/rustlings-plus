use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chat_service::{Config, Line, run};
use grpc_kit::Stamp;
use proto::domain_code;
use proto::social::v1::chat_client::ChatClient;
use proto::social::v1::chat_server::SERVICE_NAME;
use proto::social::v1::users_client::UsersClient;
use proto::social::v1::{CreateProfileRequest, HistoryRequest, SendRequest, SubscribeRequest};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tonic::Code;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_check_response::ServingStatus;
use tonic_health::pb::health_client::HealthClient;

const WAIT: Duration = Duration::from_secs(5);

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
    async fn start(journal: Option<&Path>) -> Stack {
        let stop = CancellationToken::new();
        let (users_listener, users) = listener().await;
        let (chat_listener, chat) = listener().await;
        let config = Config {
            users: users.clone(),
            journal: journal.map(Path::to_path_buf),
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
    assert_eq!(stack.send(1, 2, "   ").await, Err(Code::FailedPrecondition));
    let anonymous = ChatClient::new(stack.chat.clone())
        .send(SendRequest {
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
async fn every_subscription_hears_its_messages() {
    let stack = Stack::start(None).await;
    let open = |user| {
        let mut client = stack.by(user);
        async move {
            client
                .subscribe(SubscribeRequest {})
                .await
                .expect("подписка открывается")
                .into_inner()
        }
    };
    let mut phone = open(2).await;
    let mut laptop = open(2).await;
    let mut other = open(3).await;
    stack.send(1, 2, "раз").await.expect("сообщение");
    for (name, stream) in [("телефон", &mut phone), ("ноутбук", &mut laptop)] {
        let message = timeout(WAIT, stream.message())
            .await
            .unwrap_or_else(|_| panic!("{name} bob не дождался сообщения"))
            .expect("поток жив")
            .expect("сообщение пришло");
        assert_eq!(
            (message.from_id, message.to_id, message.text.as_str()),
            (1, 2, "раз"),
            "{name}: каждое открытое Subscribe получает сообщения своего пользователя"
        );
    }
    drop(phone);
    stack
        .send(1, 2, "два")
        .await
        .expect("отправка не ломается от брошенного потока");
    let message = timeout(WAIT, laptop.message())
        .await
        .expect("ноутбук ждёт второе")
        .expect("поток жив")
        .expect("сообщение");
    assert_eq!(message.text, "два");
    stack.send(1, 3, "carol").await.expect("сообщение carol");
    let mine = timeout(WAIT, other.message())
        .await
        .expect("carol ждёт своё")
        .expect("поток жив")
        .expect("сообщение");
    assert_eq!(
        mine.text, "carol",
        "чужие сообщения в поток не попадают, своё приходит"
    );
    stack.stop().await;
}

#[tokio::test]
async fn journal_survives_restart_and_a_torn_tail() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m31-chat");
    let _ = std::fs::remove_dir_all(&dir);
    let journal = dir.join("chat.ndjson");
    let stack = Stack::start(Some(&journal)).await;
    stack.send(1, 2, "до перезапуска").await.expect("сообщение");
    stack.stop().await;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .expect("журнал на диске");
    file.write_all(br#"{"from": 2, "to": 1, "te"#)
        .expect("обрывок");
    drop(file);
    let stack = Stack::start(Some(&journal)).await;
    assert_eq!(
        stack.history(1, 2).await,
        [(1, "до перезапуска".to_string())],
        "переписка поднимается из журнала, оборванная строка отрезается"
    );
    stack.send(2, 1, "после").await.expect("новое сообщение");
    stack.stop().await;
    let text = std::fs::read_to_string(&journal).expect("журнал читается");
    let lines: Vec<Line> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("строка «{line}»: {e}")))
        .collect();
    assert_eq!(
        lines,
        [
            Line {
                from: 1,
                to: 2,
                text: "до перезапуска".to_string()
            },
            Line {
                from: 2,
                to: 1,
                text: "после".to_string()
            },
        ]
    );
}
