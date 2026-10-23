use auth_service::{Config, run};
use contracts::{ErrorBody, Login, Outcome, decode};
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

#[tokio::test]
async fn refresh_lives_in_an_http_only_cookie_and_rotates() {
    let stack = Stack::start().await;
    let weak = stack
        .post(
            "/auth/register",
            r#"{"username": "alice", "password": "short"}"#,
            &[],
        )
        .await;
    assert_eq!(
        (weak.status, weak.error().fields()),
        (422, vec![("password", "too_short")]),
        "пароль короче 8 символов не принимается"
    );
    let same = stack
        .post(
            "/auth/register",
            r#"{"username": "alice", "password": "Alice"}"#,
            &[],
        )
        .await;
    assert!(
        same.error()
            .fields()
            .iter()
            .any(|(field, _)| *field == "password")
    );
    assert_eq!(stack.post("/auth/register", ALICE, &[]).await.status, 201);
    let login = stack.post("/auth/login", ALICE, &[]).await;
    assert_eq!(login.status, 200, "{}", login.body);
    let body: Login = decode(&login.body).expect("Login");
    assert_eq!(body.expires_in, 900);
    assert!(
        !login.body.contains("refresh"),
        "refresh не уходит в тело: скрипт страницы его не прочитает"
    );
    let refresh_cookie = login
        .cookies()
        .into_iter()
        .find(|cookie| cookie.starts_with("refresh="))
        .expect("cookie refresh")
        .to_string();
    for flag in ["HttpOnly", "Path=/auth", "SameSite=Strict"] {
        assert!(refresh_cookie.contains(flag), "{flag} в {refresh_cookie}");
    }
    let first = login.cookie("refresh").expect("refresh");
    let renewed = stack
        .post(
            "/auth/refresh",
            "",
            &[("Cookie", &format!("refresh={first}"))],
        )
        .await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
    let second = renewed.cookie("refresh").expect("новый refresh");
    assert_ne!(second, first, "ротация: каждый обмен даёт новый refresh");
    let replay = stack
        .post(
            "/auth/refresh",
            "",
            &[("Cookie", &format!("refresh={first}"))],
        )
        .await;
    assert_eq!(replay.status, 401);
    assert_eq!(
        replay.cookie("refresh").as_deref(),
        Some(""),
        "cookie стирается"
    );
    let burned = stack
        .post(
            "/auth/refresh",
            "",
            &[("Cookie", &format!("refresh={second}"))],
        )
        .await;
    assert_eq!(
        burned.status, 401,
        "повтор погасил всё семейство, и свежий refresh тоже"
    );
    let none = stack.post("/auth/refresh", "", &[]).await;
    assert_eq!(none.status, 401);
    stack.stop().await;
}

#[tokio::test]
async fn logout_everywhere_over_http() {
    let stack = Stack::start().await;
    assert_eq!(stack.post("/auth/register", ALICE, &[]).await.status, 201);
    let laptop: Login = decode(&stack.post("/auth/login", ALICE, &[]).await.body).expect("Login");
    let phone = stack.post("/auth/login", ALICE, &[]).await;
    let phone_refresh = phone.cookie("refresh").expect("refresh");
    let bearer = format!("Bearer {}", laptop.token);
    let out = stack
        .post("/auth/logout-all", "", &[("Authorization", &bearer)])
        .await;
    assert_eq!(out.status, 204);
    let again = stack
        .post(
            "/auth/refresh",
            "",
            &[("Cookie", &format!("refresh={phone_refresh}"))],
        )
        .await;
    assert_eq!(again.status, 401, "второе устройство тоже вышло");
    let stale = stack
        .post("/auth/logout", "", &[("Authorization", &bearer)])
        .await;
    assert_eq!(
        (stale.status, stale.error().code.as_str()),
        (401, "unauthorized"),
        "погашенный access не принимается и на защищённых маршрутах auth"
    );
    stack.stop().await;
}
