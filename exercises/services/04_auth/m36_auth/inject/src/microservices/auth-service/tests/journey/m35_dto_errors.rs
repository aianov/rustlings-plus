use std::path::PathBuf;

use auth_service::{Config, run};
use contracts::{ErrorBody, Outcome, UserView, decode};
use service_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

struct Reply {
    status: u16,
    head: String,
    body: String,
}

impl Reply {
    fn error(&self) -> ErrorBody {
        match decode::<Outcome<()>>(&self.body) {
            Ok(Outcome::Error(body)) => body,
            other => panic!("ждали конверт ошибки, а пришло {other:?}: {}", self.body),
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

struct Stack {
    http: String,
    journal: PathBuf,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    (listener, addr)
}

impl Stack {
    async fn start(name: &str) -> Stack {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("m35")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let journal = dir.join("auth.ndjson");
        let stop = CancellationToken::new();
        let (users, users_addr) = listener().await;
        let (http, http_addr) = listener().await;
        let (grpc, _) = listener().await;
        let users_stop = stop.clone();
        let users_task = tokio::spawn(async move {
            let _ = user_service::run(users, user_service::Config::default(), users_stop).await;
        });
        let config = Config {
            users: users_addr,
            journal: Some(journal.clone()),
            ..Config::default()
        };
        let auth_stop = stop.clone();
        let auth_task = tokio::spawn(async move {
            let _ = run(http, grpc, config, auth_stop).await;
        });
        Stack {
            http: http_addr,
            journal,
            stop,
            tasks: vec![users_task, auth_task],
        }
    }

    async fn post(&self, path: &str, content_type: &str, body: &str) -> Reply {
        let mut stream = TcpStream::connect(&self.http).await.expect("HTTP");
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: auth\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.expect("запрос");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("ответ");
        let text = String::from_utf8_lossy(&raw).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        Reply {
            status,
            head: head.to_string(),
            body: body.to_string(),
        }
    }

    async fn register(&self, body: &str) -> Reply {
        self.post("/auth/register", "application/json", body).await
    }

    async fn stop(self) {
        self.stop.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

#[tokio::test]
async fn registration_reports_every_field_at_once() {
    let stack = Stack::start("register-fields").await;
    let both = stack
        .register(r#"{"username": "ab", "password": "correct-horse", "email": "not-an-email"}"#)
        .await;
    assert_eq!(both.status, 422, "{}", both.body);
    let error = both.error();
    assert_eq!(error.code, "validation");
    assert_eq!(
        error.fields(),
        [("username", "too_short"), ("email", "invalid_format")],
        "ник и почта проверены за один запрос"
    );
    let rid = error.request_id.clone().expect("номер запроса в теле");
    assert_eq!(
        both.header("x-request-id"),
        Some(rid.as_str()),
        "тот же номер в заголовке: по нему поддержка найдёт запись в логе"
    );
    let created = stack
        .register(
            r#"{"username": " Alice ", "password": "correct-horse", "email": "Alice@Example.com"}"#,
        )
        .await;
    assert_eq!(created.status, 201, "{}", created.body);
    let alice: UserView = decode(&created.body).expect("UserView");
    assert_eq!(alice.username, "alice");
    let journal = std::fs::read_to_string(&stack.journal).expect("журнал учётных записей");
    assert!(
        journal.contains(r#""email":"alice@example.com""#),
        "почта сохранена нормализованной: {journal}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn broken_bodies_are_400_and_shapes_are_422() {
    let stack = Stack::start("register-bodies").await;
    let broken = stack.register("{not json").await;
    assert_eq!(
        (broken.status, broken.error().code.as_str()),
        (400, "syntax")
    );
    let cut = stack.register(r#"{"username": "ali"#).await;
    assert_eq!((cut.status, cut.error().code.as_str()), (400, "truncated"));
    let typed = stack
        .register(r#"{"username": "hunter2", "password": "correct-horse", "email": 42}"#)
        .await;
    assert_eq!(typed.status, 422);
    assert_eq!(typed.error().fields(), [("", "wrong_type")]);
    let missing = stack.register(r#"{"email": "a@b.cd"}"#).await;
    assert_eq!(missing.error().fields(), [("username", "missing")]);
    let sneaky = stack
        .register(r#"{"username": "mallory", "admin": true}"#)
        .await;
    assert_eq!(
        (sneaky.status, sneaky.error().fields()),
        (422, vec![("admin", "unknown_field")]),
        "лишнее поле при регистрации это отказ, а не тихое «admin: true»"
    );
    let text = stack
        .post(
            "/auth/register",
            "text/plain",
            r#"{"username": "alice", "password": "correct-horse"}"#,
        )
        .await;
    assert_eq!(
        (text.status, text.error().code.as_str()),
        (415, "unsupported")
    );
    let login = stack
        .post(
            "/auth/login",
            "application/json",
            r#"{"username": "a b", "password": "correct-horse"}"#,
        )
        .await;
    assert_eq!(
        (login.status, login.error().fields()),
        (422, vec![("username", "invalid_char")])
    );
    for reply in [&broken, &typed, &sneaky, &login] {
        assert!(
            !reply.body.contains("hunter2") && !reply.body.contains("invalid type"),
            "в ответе нет ни присланных значений, ни текста serde: {}",
            reply.body
        );
        assert!(reply.error().request_id.is_some(), "{}", reply.body);
    }
    stack.stop().await;
}
