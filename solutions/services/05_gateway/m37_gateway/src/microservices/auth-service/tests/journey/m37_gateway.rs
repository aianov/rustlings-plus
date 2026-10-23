use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use auth_service::{Config, run};
use contracts::{ErrorBody, Login, Outcome, decode};
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::{GetAccountRequest, ValidateTokenRequest};
use service_runtime::CancellationToken;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт");
    let addr = listener.local_addr().expect("адрес");
    (listener, addr)
}

struct Stack {
    http: SocketAddr,
    grpc: SocketAddr,
    stop: CancellationToken,
}

impl Stack {
    async fn start() -> Stack {
        let stop = CancellationToken::new();
        let (users, users_addr) = listener().await;
        let (http, http_addr) = listener().await;
        let (grpc, grpc_addr) = listener().await;
        tokio::spawn(user_service::run(
            users,
            user_service::Config::default(),
            stop.clone(),
        ));
        let config = Config {
            users: users_addr.to_string(),
            ..Config::default()
        };
        tokio::spawn(run(http, grpc, config, stop.clone()));
        Stack {
            http: http_addr,
            grpc: grpc_addr,
            stop,
        }
    }

    async fn post(&self, path: &str, body: &str, rid: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(self.http).await.expect("HTTP");
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: auth\r\nX-Request-Id: {rid}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.expect("запрос");
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.expect("ответ");
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        (status, body.to_string())
    }
}

const ALICE: &str = r#"{"username": "alice", "password": "correct-horse"}"#;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn refused(body: &str) -> ErrorBody {
    match decode::<Outcome<()>>(body) {
        Ok(Outcome::Error(body)) => body,
        other => panic!("ждали отказ: {other:?}: {body}"),
    }
}

#[tokio::test]
async fn validate_token_tells_the_gateway_when_to_close() {
    let stack = Stack::start().await;
    let mut ready = false;
    for _ in 0..100 {
        if stack.post("/auth/register", ALICE, "gw-1").await.0 == 201 {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(ready, "регистрация проходит");
    let (status, body) = stack.post("/auth/login", ALICE, "gw-2").await;
    assert_eq!(status, 200, "{body}");
    let login: Login = decode(&body).expect("вход");
    let mut auth = AuthClient::connect(format!("http://{}", stack.grpc))
        .await
        .expect("gRPC auth-service");
    let account = auth
        .validate_token(ValidateTokenRequest { token: login.token })
        .await
        .expect("токен годен")
        .into_inner();
    let expected = now() + login.expires_in;
    assert!(
        account.expires_at.abs_diff(expected) <= 2,
        "ValidateToken отдаёт срок токена в секундах Unix: {} против {expected}",
        account.expires_at
    );
    let plain = auth
        .get_account(GetAccountRequest {
            user_id: account.user_id,
        })
        .await
        .expect("учётная запись")
        .into_inner();
    assert_eq!(plain.expires_at, 0, "у учётной записи срока нет");
    stack.stop.cancel();
}

#[tokio::test]
async fn the_gateway_request_id_is_kept_and_a_bad_one_is_replaced() {
    let stack = Stack::start().await;
    let wrong = r#"{"username": "alice", "password": "wrong-horse"}"#;
    let mut kept = None;
    for _ in 0..100 {
        let (status, body) = stack.post("/auth/login", wrong, "gw-7f3a.1").await;
        if status == 401 {
            kept = refused(&body).request_id;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        kept.as_deref(),
        Some("gw-7f3a.1"),
        "за шлюзом номер запроса один на все записи: auth-service берёт x-request-id шлюза"
    );
    let (_, body) = stack.post("/auth/login", wrong, "bad id!").await;
    let fresh = refused(&body).request_id.unwrap_or_default();
    assert!(
        fresh != "bad id!" && fresh.len() == 16,
        "кривой номер заменяется своим: {fresh:?}"
    );
    stack.stop.cancel();
}
