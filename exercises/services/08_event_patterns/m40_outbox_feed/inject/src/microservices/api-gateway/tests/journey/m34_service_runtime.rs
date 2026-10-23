use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use api_gateway::{ADMIN_ENV, BUDGET_ENV, Config, MAX_CONNECTIONS_ENV};
use contracts::{ReadyView, decode};
use service_runtime::Source;
use social_cli::{Web, await_ready};
use tokio::time::sleep;

use crate::support::{DEADLINE, Gateway, issue_certs, listener, within};

async fn ready(admin: &str) -> (u16, ReadyView) {
    let reply = Web::new(admin)
        .request("GET", "/ready", None, None)
        .await
        .expect("admin-порт шлюза отвечает");
    let view = decode(&reply.body).unwrap_or_else(|e| panic!("/ready это JSON ({e}): {reply:?}"));
    (reply.status, view)
}

#[test]
fn config_reads_the_services_and_limits_and_rejects_nonsense() {
    let source = Source::from_pairs([
        ("POSTS_GRPC_ADDR", "10.0.0.3:50053"),
        (MAX_CONNECTIONS_ENV, "1000"),
        (BUDGET_ENV, "750"),
        (
            "GATEWAY_CORS_ORIGINS",
            "https://social.example, http://localhost:5173",
        ),
    ]);
    let config = Config::from_source(&source).expect("всё верно");
    assert_eq!(config.services.posts, "10.0.0.3:50053");
    assert_eq!(config.services.users, "127.0.0.1:50052");
    assert_eq!(config.max_connections, 1000);
    assert_eq!(config.budget, Duration::from_millis(750));
    assert_eq!(
        config.cors_origins,
        ["https://social.example", "http://localhost:5173"]
    );
    assert_eq!(config.admin, "127.0.0.1:9880");
    let bad = Source::from_pairs([
        ("CHAT_GRPC_ADDR", "chat"),
        (MAX_CONNECTIONS_ENV, "0"),
        (BUDGET_ENV, "0"),
        (ADMIN_ENV, "127.0.0.1:7880"),
    ]);
    let error = Config::from_source(&bad).expect_err("четыре ошибки");
    assert_eq!(
        error.keys(),
        ["CHAT_GRPC_ADDR", MAX_CONNECTIONS_ENV, BUDGET_ENV, ADMIN_ENV],
        "{error}"
    );
}

#[tokio::test]
async fn ready_names_the_service_that_went_down() {
    let mut stack = Gateway::with_admin(Config::default()).await;
    let admin = stack.admin.expect("шлюз поднят с admin-портом").to_string();
    let (status, view) = ready(&admin).await;
    assert_eq!(status, 200, "{view:?}");
    let names: Vec<&str> = view
        .checks
        .iter()
        .map(|check| check.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "auth-service",
            "user-service",
            "post-service",
            "chat-service",
            "feed-service"
        ]
    );
    stack.halt("post-service").await;
    let started = Instant::now();
    let view = loop {
        let (status, view) = ready(&admin).await;
        if status == 503 {
            break view;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "шлюз так и не заметил пропажу post-service"
        );
        sleep(Duration::from_millis(20)).await;
    };
    let failing: Vec<&str> = view
        .failing()
        .iter()
        .map(|check| check.name.as_str())
        .collect();
    assert_eq!(failing, ["post-service"], "{view:?}");
    let live = Web::new(&admin)
        .request("GET", "/health", None, None)
        .await
        .expect("admin");
    assert_eq!(live.status, 200, "шлюз жив, перезапускать его бесполезно");
    let mut screen = Vec::new();
    let became_ready = within(
        "клиент ждёт готовности",
        await_ready(&admin, Duration::from_secs(1), &mut screen),
    )
    .await
    .expect("клиент пишет на экран");
    let screen = String::from_utf8(screen).expect("UTF-8");
    assert!(!became_ready);
    assert!(
        screen.contains("не готов post-service")
            && screen.contains("не дождался готовности за 1 с"),
        "клиент называет неготовый сервис:\n{screen}"
    );
    stack.stop().await;
}

#[tokio::test]
async fn the_client_starts_as_soon_as_the_gateway_is_ready() {
    let stack = Gateway::with_admin(Config::default()).await;
    let admin = stack.admin.expect("admin-порт").to_string();
    let mut screen = Vec::new();
    let became_ready = within(
        "клиент ждёт готовности",
        await_ready(&admin, Duration::from_secs(5), &mut screen),
    )
    .await
    .expect("клиент пишет на экран");
    let screen = String::from_utf8(screen).expect("UTF-8");
    assert!(became_ready, "{screen}");
    assert!(screen.contains("шлюз готов"), "{screen}");
    stack.stop().await;
    let mut screen = Vec::new();
    let silent = await_ready(&admin, Duration::from_millis(300), &mut screen)
        .await
        .expect("клиент пишет на экран");
    let screen = String::from_utf8(screen).expect("UTF-8");
    assert!(!silent);
    assert!(
        screen.contains("молчит"),
        "шлюз не запущен, и клиент так и говорит:\n{screen}"
    );
}

#[tokio::test]
async fn the_gateway_binary_without_services_gives_up_naming_them() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m34-gateway-binary");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("папка");
    let certs = issue_certs("m34-gateway-binary");
    let mut closed = Vec::new();
    for _ in 0..4 {
        let (socket, addr) = listener().await;
        drop(socket);
        closed.push(addr.to_string());
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_api-gateway"))
        .current_dir(&dir)
        .env_clear()
        .env("GATEWAY_HTTP_ADDR", "127.0.0.1:0")
        .env(ADMIN_ENV, "127.0.0.1:0")
        .env("AUTH_GRPC_ADDR", &closed[0])
        .env("USERS_GRPC_ADDR", &closed[1])
        .env("POSTS_GRPC_ADDR", &closed[2])
        .env("CHAT_GRPC_ADDR", &closed[3])
        .env("CERTS_DIR", &certs)
        .env("STARTUP_WAIT_MS", "800")
        .env("STARTUP_RETRY_MS", "100")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("бинарь шлюза запускается");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("статус") {
            break status;
        }
        if started.elapsed() > DEADLINE {
            let _ = child.kill();
            panic!("шлюз без сервисов ждёт вечно, а дедлайн 800 мс");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut stderr)
        .expect("stderr читается");
    assert!(!status.success(), "{stderr}");
    for service in [
        "auth-service",
        "user-service",
        "post-service",
        "chat-service",
    ] {
        assert!(
            stderr.contains(service),
            "в причине назван {service}:\n{stderr}"
        );
    }
    assert!(stderr.contains("800 мс"), "{stderr}");
}
