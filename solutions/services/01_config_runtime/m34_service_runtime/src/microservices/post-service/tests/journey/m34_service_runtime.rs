use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use post_service::{ADMIN_ENV, Config, serve};
use service_runtime::{CancellationToken, Source};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

const LIMIT: Duration = Duration::from_secs(10);

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m34")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("временная папка");
    dir
}

async fn get(addr: &str, path: &str) -> (u16, String) {
    let Ok(mut stream) = TcpStream::connect(addr).await else {
        return (0, String::new());
    };
    let ask = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    if stream.write_all(ask.as_bytes()).await.is_err() {
        return (0, String::new());
    }
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw).await;
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

async fn eventually(addr: &str, path: &str, want: u16, needle: &str) -> String {
    let started = Instant::now();
    loop {
        let (status, body) = get(addr, path).await;
        if status == want && body.contains(needle) {
            return body;
        }
        assert!(
            started.elapsed() < LIMIT,
            "{path} так и не ответил {want} с «{needle}», последнее: {status} {body}"
        );
        sleep(Duration::from_millis(20)).await;
    }
}

async fn port() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("порт 0");
    let addr = listener.local_addr().expect("адрес").to_string();
    (listener, addr)
}

#[test]
fn config_reads_its_neighbour_and_rejects_a_bad_one() {
    let config = Config::from_source(&Source::new().with("USERS_GRPC_ADDR", "10.0.0.7:50052"))
        .expect("адрес соседа верный");
    assert_eq!(config.users, "10.0.0.7:50052");
    assert_eq!(config.addr, "127.0.0.1:50053");
    assert_eq!(config.admin, "127.0.0.1:9883");
    let error = Config::from_source(
        &Source::new()
            .with("USERS_GRPC_ADDR", "user-service")
            .with("POSTS_GRPC_ADDR", "127.0.0.1:9883"),
    )
    .expect_err("без порта и занятый admin-порт");
    assert_eq!(error.keys(), ["USERS_GRPC_ADDR", ADMIN_ENV]);
}

#[tokio::test]
async fn ready_follows_user_service_and_live_does_not() {
    let (users, users_addr) = port().await;
    let users_stop = CancellationToken::new();
    let users_task = tokio::spawn(user_service::run(
        users,
        user_service::Config::default(),
        users_stop.clone(),
    ));
    let (posts, _) = port().await;
    let (admin, admin_addr) = port().await;
    let stop = CancellationToken::new();
    let config = Config {
        users: users_addr,
        ..Config::default()
    };
    let posts_task = tokio::spawn(serve(posts, admin, config, stop.clone()));
    let body = eventually(&admin_addr, "/ready", 200, "user-service").await;
    assert!(body.contains("\"journal\""), "{body}");
    users_stop.cancel();
    users_task
        .await
        .expect("user-service")
        .expect("останавливается чисто");
    let body = eventually(&admin_addr, "/ready", 503, "user-service").await;
    assert!(
        body.contains("\"ok\":false"),
        "без user-service пост не создать: экземпляр снимают с трафика: {body}"
    );
    let (status, _) = get(&admin_addr, "/health").await;
    assert_eq!(
        status, 200,
        "/health не зависит от соседей: перезапуск post-service не поднимет user-service"
    );
    stop.cancel();
    posts_task
        .await
        .expect("post-service")
        .expect("останавливается чисто без соседа");
}

struct Process {
    child: Child,
    lines: Receiver<String>,
    errors: Arc<Mutex<String>>,
}

impl Process {
    fn spawn(dir: &Path, vars: &[(&str, &str)]) -> Process {
        let mut child = Command::new(env!("CARGO_BIN_EXE_post-service"))
            .current_dir(dir)
            .env_clear()
            .envs(vars.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("бинарь post-service запускается");
        let stdout = child.stdout.take().expect("stdout");
        let mut stderr = child.stderr.take().expect("stderr");
        let (send, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = send.send(line);
            }
        });
        let errors = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&errors);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 1024];
            while let Ok(read) = stderr.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&chunk[..read]).to_string();
                sink.lock().expect("stderr").push_str(&text);
            }
        });
        Process {
            child,
            lines,
            errors,
        }
    }

    fn admin(&self) -> String {
        let banner = self
            .lines
            .recv_timeout(LIMIT)
            .unwrap_or_else(|_| panic!("нет строки о запуске, stderr: {}", self.stderr()));
        let start = banner
            .find("admin http://")
            .expect("admin-адрес в строке запуска")
            + "admin http://".len();
        let rest = &banner[start..];
        rest[..rest.find('/').expect("адрес до /ready")].to_string()
    }

    fn stderr(&self) -> String {
        self.errors.lock().expect("stderr").clone()
    }

    fn wait(&mut self, limit: Duration) -> ExitStatus {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().expect("статус процесса") {
                std::thread::sleep(Duration::from_millis(50));
                return status;
            }
            assert!(
                started.elapsed() < limit,
                "процесс не завершился за {limit:?}, stderr: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn closed_port() -> String {
    let (listener, addr) = port().await;
    drop(listener);
    addr
}

fn certs(name: &str) -> PathBuf {
    let dir = scratch(name).join("certs");
    certgen::issue_all(&dir, false).expect("сертификаты");
    dir
}

#[tokio::test]
async fn the_binary_waits_for_user_service_and_then_names_it() {
    let dir = scratch("posts-wait");
    let certs = certs("posts-wait-certs");
    let certs = certs.to_string_lossy();
    let users = closed_port().await;
    let mut process = Process::spawn(
        &dir,
        &[
            ("POSTS_GRPC_ADDR", "127.0.0.1:0"),
            (ADMIN_ENV, "127.0.0.1:0"),
            ("USERS_GRPC_ADDR", &users),
            ("CERTS_DIR", &certs),
            ("STARTUP_WAIT_MS", "1500"),
            ("STARTUP_RETRY_MS", "100"),
        ],
    );
    let admin = process.admin();
    let body = eventually(&admin, "/ready", 503, "user-service").await;
    assert!(
        body.contains("\"ok\":false"),
        "пока ждёт, /ready честно говорит, кого: {body}"
    );
    let (status, _) = get(&admin, "/health").await;
    assert_eq!(
        status, 200,
        "ожидание соседа это не повод перезапускать процесс"
    );
    let status = process.wait(LIMIT);
    assert!(
        !status.success(),
        "соседа нет и после дедлайна: падение с причиной"
    );
    let stderr = process.stderr();
    assert!(
        stderr.contains("так и не готовы: user-service"),
        "сообщение называет, кого не дождались: {stderr}"
    );
    assert!(stderr.contains("1500"), "и сколько ждали: {stderr}");
}

#[tokio::test]
async fn sigterm_during_the_wait_exits_cleanly() {
    let dir = scratch("posts-sigterm");
    let certs = certs("posts-sigterm-certs");
    let certs = certs.to_string_lossy();
    let users = closed_port().await;
    let mut process = Process::spawn(
        &dir,
        &[
            ("POSTS_GRPC_ADDR", "127.0.0.1:0"),
            (ADMIN_ENV, "127.0.0.1:0"),
            ("USERS_GRPC_ADDR", &users),
            ("CERTS_DIR", &certs),
            ("STARTUP_WAIT_MS", "60000"),
        ],
    );
    let admin = process.admin();
    eventually(&admin, "/ready", 503, "user-service").await;
    let sent = Command::new("kill")
        .args(["-TERM", &process.child.id().to_string()])
        .status()
        .expect("kill");
    assert!(sent.success());
    let started = Instant::now();
    let status = process.wait(LIMIT);
    assert!(status.success(), "{status:?}: {}", process.stderr());
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "минута ожидания не держит остановку"
    );
    let stderr = process.stderr();
    assert!(
        stderr.contains("получен SIGTERM") && stderr.contains("остановлен"),
        "{stderr}"
    );
}
