use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use auth_service::{Account, Config, SESSION_COOKIE, run};
use contracts::{Login, UserView, decode};
use proto::social::v1::ValidateTokenRequest;
use proto::social::v1::auth_client::AuthClient;
use service_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tonic::Code;

async fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("порт 0 свободен всегда");
    let addr = listener.local_addr().expect("у слушателя есть адрес");
    (listener, addr)
}

struct Reply {
    status: u16,
    head: String,
    body: String,
}

async fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut stream = TcpStream::connect(addr)
        .await
        .expect("HTTP auth-service отвечает");
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str(&format!(
        "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ));
    stream
        .write_all(raw.as_bytes())
        .await
        .expect("запрос уходит");
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .await
        .expect("ответ читается");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or((answer.as_str(), ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("строка статуса");
    Reply {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

struct Users {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<user_service::Result<()>>,
}

impl Users {
    async fn start() -> Users {
        let (listener, addr) = listener().await;
        let stop = CancellationToken::new();
        let task = tokio::spawn(user_service::run(
            listener,
            user_service::Config::default(),
            stop.clone(),
        ));
        Users { addr, stop, task }
    }

    async fn stop(self) {
        self.stop.cancel();
        assert!(self.task.await.expect("задача не падает").is_ok());
    }
}

struct Auth {
    http: SocketAddr,
    grpc: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<auth_service::Result<()>>,
}

impl Auth {
    async fn start(users: String, journal: Option<&Path>) -> Auth {
        let (web, http) = listener().await;
        let (rpc, grpc) = listener().await;
        let stop = CancellationToken::new();
        let config = Config {
            users,
            journal: journal.map(Path::to_path_buf),
            ..Config::default()
        };
        let task = tokio::spawn(run(web, rpc, config, stop.clone()));
        Auth {
            http,
            grpc,
            stop,
            task,
        }
    }

    async fn post(&self, path: &str, body: &str) -> Reply {
        http(self.http, "POST", path, &[], body).await
    }

    async fn login(&self, name: &str) -> Login {
        let reply = self
            .post("/auth/login", &format!(r#"{{"username": "{name}"}}"#))
            .await;
        assert_eq!(reply.status, 200, "вход {name}: {}", reply.body);
        decode(&reply.body).expect("ответ входа разбирается")
    }

    async fn valid(&self, token: &str) -> Result<u64, Code> {
        let mut client = AuthClient::connect(format!("http://{}", self.grpc))
            .await
            .expect("gRPC auth-service отвечает");
        let ask = ValidateTokenRequest {
            token: token.to_string(),
        };
        match client.validate_token(ask).await {
            Ok(account) => Ok(account.into_inner().user_id),
            Err(status) => Err(status.code()),
        }
    }

    async fn stop(self) {
        self.stop.cancel();
        let finished = self.task.await.expect("задача auth-service не падает");
        assert!(
            finished.is_ok(),
            "auth-service останавливается без ошибки: {finished:?}"
        );
    }
}

#[tokio::test]
async fn register_login_logout_over_http_and_tokens_over_grpc() {
    let users = Users::start().await;
    let auth = Auth::start(users.addr.to_string(), None).await;
    let created = auth
        .post("/auth/register", r#"{"username": "Alice"}"#)
        .await;
    assert_eq!(created.status, 201, "{}", created.body);
    let alice: UserView = decode(&created.body).expect("UserView разбирается");
    assert_eq!(alice.username, "alice", "ник нормализует user-service");
    let crooked = auth.post("/auth/register", r#"{"username": "a b"}"#).await;
    assert_eq!(crooked.status, 400, "кривой ник отсекается ещё в HTTP");
    let login = auth.login("ALICE").await;
    assert_eq!(login.user, alice);
    let reply = auth.post("/auth/login", r#"{"username": "alice"}"#).await;
    assert!(
        reply.head.contains(&format!("{SESSION_COOKIE}=")) && reply.head.contains("HttpOnly"),
        "вход ставит cookie сессии: {}",
        reply.head
    );
    assert_eq!(auth.valid(&login.token).await, Ok(alice.id.0));
    let anonymous = auth.post("/auth/logout", "").await;
    assert_eq!(anonymous.status, 401, "выход только с токеном");
    let bearer = format!("Bearer {}", login.token);
    let out = http(
        auth.http,
        "POST",
        "/auth/logout",
        &[("Authorization", &bearer)],
        "",
    )
    .await;
    assert_eq!(out.status, 204);
    assert_eq!(
        auth.valid(&login.token).await,
        Err(Code::Unauthenticated),
        "отозванный токен больше не проходит ValidateToken"
    );
    let health = http(auth.http, "GET", "/health", &[], "").await;
    assert_eq!(health.status, 200);
    auth.stop().await;
    users.stop().await;
}

#[tokio::test]
async fn accounts_survive_restart_and_a_torn_tail() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m30-auth");
    let _ = std::fs::remove_dir_all(&dir);
    let journal = dir.join("accounts.ndjson");
    let users = Users::start().await;
    let auth = Auth::start(users.addr.to_string(), Some(&journal)).await;
    for name in ["alice", "bob"] {
        let reply = auth
            .post("/auth/register", &format!(r#"{{"username": "{name}"}}"#))
            .await;
        assert_eq!(reply.status, 201, "{}", reply.body);
    }
    auth.stop().await;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .expect("журнал учётных записей есть на диске");
    file.write_all(br#"{"user_id": 3, "usern"#)
        .expect("обрывок дописывается");
    drop(file);

    let auth = Auth::start(users.addr.to_string(), Some(&journal)).await;
    let bob = auth.login("bob").await;
    assert_eq!(bob.user.id.0, 2, "учётные записи поднялись из журнала");
    let carol = auth
        .post("/auth/register", r#"{"username": "carol"}"#)
        .await;
    assert_eq!(carol.status, 201, "{}", carol.body);
    auth.stop().await;
    let text = std::fs::read_to_string(&journal).expect("журнал читается");
    let accounts: Vec<Account> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("строка «{line}»: {e}")))
        .collect();
    let names: Vec<&str> = accounts.iter().map(|a| a.username.as_str()).collect();
    assert_eq!(
        names,
        ["alice", "bob", "carol"],
        "оборванная запись при старте отрезается, новая ложится целой строкой"
    );
    let auth = Auth::start(users.addr.to_string(), Some(&journal)).await;
    assert_eq!(auth.login("carol").await.user.id.0, 3);
    auth.stop().await;
    users.stop().await;
}

#[tokio::test]
async fn fallen_user_service_is_503_on_register() {
    let (dead, addr) = listener().await;
    drop(dead);
    let auth = Auth::start(addr.to_string(), None).await;
    let reply = auth
        .post("/auth/register", r#"{"username": "alice"}"#)
        .await;
    assert!(
        reply.status == 503 && reply.body.contains("unavailable"),
        "user-service лежит: 503 unavailable, а не 500: {} {}",
        reply.status,
        reply.body
    );
    let unknown = auth.post("/auth/login", r#"{"username": "alice"}"#).await;
    assert_eq!(unknown.status, 401, "незарегистрированный не входит");
    auth.stop().await;
}
