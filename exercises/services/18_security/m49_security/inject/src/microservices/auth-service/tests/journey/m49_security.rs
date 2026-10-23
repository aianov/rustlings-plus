use std::time::{Duration, Instant};

use auth_service::{Config, Throttle, ThrottleKey, run};
use contracts::{ErrorBody, Outcome, decode};
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
    fn cookies(&self) -> Vec<&str> {
        self.head
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, value)| value.trim())
            .collect()
    }

    fn cookie(&self, name: &str) -> Option<String> {
        self.cookies().into_iter().find_map(|cookie| {
            let pair = cookie.split(';').next()?;
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| value.to_string())
        })
    }

    fn error(&self) -> ErrorBody {
        match decode::<Outcome<()>>(&self.body) {
            Ok(Outcome::Error(body)) => body,
            other => panic!("ждали конверт ошибки, а пришло {other:?}: {}", self.body),
        }
    }
}

struct Stack {
    http: String,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес").to_string();
    (listener, addr)
}

impl Stack {
    async fn start() -> Stack {
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
            ..Config::default()
        };
        let auth_stop = stop.clone();
        let auth_task = tokio::spawn(async move {
            let _ = run(http, grpc, config, auth_stop).await;
        });
        Stack {
            http: http_addr,
            stop,
            tasks: vec![users_task, auth_task],
        }
    }

    async fn post(&self, path: &str, body: &str, headers: &[(&str, &str)]) -> Reply {
        let mut stream = TcpStream::connect(&self.http).await.expect("HTTP");
        let mut request = format!(
            "POST {path} HTTP/1.1\r\nHost: auth\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
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

    async fn stop(self) {
        self.stop.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

const ALICE: &str = r#"{"username": "alice", "password": "correct-horse"}"#;

const WRONG: &str = r#"{"username": "alice", "password": "wrong-horse!"}"#;

const GHOST: &str = r#"{"username": "ghost", "password": "wrong-horse!"}"#;

#[tokio::test(start_paused = true)]
async fn delay_grows_per_key_and_resets_on_success() {
    let throttle = Throttle::new(
        Duration::from_millis(100),
        Duration::from_secs(1),
        Duration::from_secs(60),
    );
    let alice = ThrottleKey::Account(String::from("alice"));
    let bob = ThrottleKey::Account(String::from("bob"));
    let home = ThrottleKey::Address("10.0.0.1".parse().expect("адрес"));
    assert_eq!(throttle.delay(&[alice.clone()]), Duration::ZERO);
    for expected in [100u64, 200, 400, 800, 1000, 1000] {
        throttle.failed(&[alice.clone(), home.clone()]);
        assert_eq!(
            throttle.delay(&[alice.clone()]),
            Duration::from_millis(expected)
        );
    }
    assert_eq!(
        throttle.delay(&[bob.clone()]),
        Duration::ZERO,
        "чужой аккаунт не страдает"
    );
    assert_eq!(
        throttle.delay(&[bob.clone(), home.clone()]),
        Duration::from_secs(1),
        "но с того же адреса тоже медленно"
    );
    throttle.succeeded("alice");
    assert_eq!(
        throttle.delay(&[alice.clone()]),
        Duration::ZERO,
        "вход хозяина снимает задержку аккаунта"
    );
    tokio::time::advance(Duration::from_secs(61)).await;
    assert_eq!(
        throttle.delay(&[home]),
        Duration::ZERO,
        "старые промахи забываются"
    );
}

#[tokio::test]
async fn login_answers_alike_and_slows_down_guessing() {
    let stack = Stack::start().await;
    assert_eq!(stack.post("/auth/register", ALICE, &[]).await.status, 201);
    let unknown = stack
        .post("/auth/login", GHOST, &[("X-Real-IP", "10.9.9.9")])
        .await;
    let wrong = stack
        .post("/auth/login", WRONG, &[("X-Real-IP", "10.9.9.8")])
        .await;
    assert_eq!(unknown.status, 401);
    assert_eq!(
        (
            unknown.status,
            unknown.error().code,
            unknown.error().message
        ),
        (wrong.status, wrong.error().code, wrong.error().message),
        "чужой ник и неверный пароль неотличимы"
    );
    for _ in 0..3 {
        let _ = stack
            .post("/auth/login", WRONG, &[("X-Real-IP", "10.0.0.7")])
            .await;
    }
    let started = Instant::now();
    let slow = stack
        .post("/auth/login", WRONG, &[("X-Real-IP", "10.0.0.7")])
        .await;
    assert_eq!(slow.status, 401);
    assert!(
        started.elapsed() >= Duration::from_millis(390),
        "четвёртая попытка ждёт: {:?}",
        started.elapsed()
    );
    let right = stack
        .post("/auth/login", ALICE, &[("X-Real-IP", "10.0.0.8")])
        .await;
    assert_eq!(
        right.status, 200,
        "аккаунт не заблокирован: хозяин входит, пусть и с задержкой"
    );
    let started = Instant::now();
    let again = stack
        .post("/auth/login", ALICE, &[("X-Real-IP", "10.0.0.8")])
        .await;
    assert_eq!(again.status, 200);
    assert!(
        started.elapsed() < Duration::from_millis(390),
        "успешный вход снял задержку"
    );
    stack.stop().await;
}

#[tokio::test]
async fn refresh_by_cookie_needs_the_client_header() {
    let stack = Stack::start().await;
    assert_eq!(stack.post("/auth/register", ALICE, &[]).await.status, 201);
    let login = stack.post("/auth/login", ALICE, &[]).await;
    let refresh = login.cookie("refresh").expect("refresh");
    let cookie = format!("refresh={refresh}");
    let forged = stack
        .post("/auth/refresh", "", &[("Cookie", &cookie)])
        .await;
    assert_eq!(
        forged.status, 403,
        "чужая страница шлёт cookie, но не свой заголовок"
    );
    assert_eq!(forged.error().code, "forbidden");
    let own = stack
        .post(
            "/auth/refresh",
            "",
            &[("Cookie", &cookie), ("X-Social-Client", "web")],
        )
        .await;
    assert_eq!(own.status, 200, "{}", own.body);
    stack.stop().await;
}
