use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grpc_kit::Tls;
use service_runtime::{Policy, Source};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use user_service::{ADMIN_ENV, Config, JOURNAL_ENV, NAME};

const LIMIT: Duration = Duration::from_secs(10);

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("m34")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("временная папка");
    dir
}

fn certs(name: &str) -> PathBuf {
    let dir = scratch(name).join("certs");
    certgen::issue_all(&dir, false).expect("сертификаты выпускаются");
    dir
}

#[test]
fn config_reads_every_setting_from_the_source() {
    let defaults = Config::from_source(&Source::new()).expect("пустой источник даёт умолчания");
    assert_eq!(defaults.addr, "127.0.0.1:50052");
    assert_eq!(defaults.admin, "127.0.0.1:9882");
    assert_eq!(defaults.journal, None);
    assert_eq!(defaults.certs, PathBuf::from("certs"));
    assert_eq!(defaults.timing.grace, Duration::from_secs(1));
    assert_eq!(
        defaults.timing.wait,
        Some(Policy::default()),
        "в бою зависимости ждут"
    );
    assert!(defaults.log.is_some(), "журнал доступа в бою включён");
    assert!(
        defaults.tls.is_none(),
        "сертификаты читает main, а не разбор конфига"
    );
    let source = Source::from_pairs([
        ("USERS_GRPC_ADDR", "0.0.0.0:6000"),
        (ADMIN_ENV, "0.0.0.0:6001"),
        (JOURNAL_ENV, "data/users.ndjson"),
        ("CERTS_DIR", "/etc/social/certs"),
        ("SHUTDOWN_GRACE_MS", "250"),
        ("SHUTDOWN_DRAIN_MS", "4000"),
        ("STARTUP_WAIT_MS", "30000"),
        ("STARTUP_RETRY_MS", "500"),
        ("ACCESS_LOG", "off"),
    ]);
    let config = Config::from_source(&source).expect("всё задано верно");
    assert_eq!(config.addr, "0.0.0.0:6000");
    assert_eq!(config.admin, "0.0.0.0:6001");
    assert_eq!(config.journal, Some(PathBuf::from("data/users.ndjson")));
    assert_eq!(config.certs, PathBuf::from("/etc/social/certs"));
    assert_eq!(config.timing.grace, Duration::from_millis(250));
    assert_eq!(config.timing.drain, Duration::from_secs(4));
    assert_eq!(
        config.timing.wait,
        Some(Policy {
            interval: Duration::from_millis(500),
            total: Duration::from_secs(30),
        })
    );
    assert!(config.log.is_none());
}

#[test]
fn config_names_every_bad_variable_at_once() {
    let source = Source::from_pairs([
        ("USERS_GRPC_ADDR", "localhost"),
        (ADMIN_ENV, "127.0.0.1:99999"),
        ("SHUTDOWN_GRACE_MS", "секунду"),
        ("ACCESS_LOG", "наверное"),
    ]);
    let error = Config::from_source(&source).expect_err("четыре ошибки");
    assert_eq!(
        error.keys(),
        [
            "USERS_GRPC_ADDR",
            ADMIN_ENV,
            "SHUTDOWN_GRACE_MS",
            "ACCESS_LOG"
        ],
        "все ошибки разом: человек правит .env один раз, а не четыре"
    );
    let clash = Source::from_pairs([
        ("USERS_GRPC_ADDR", "127.0.0.1:7000"),
        (ADMIN_ENV, "127.0.0.1:7000"),
    ]);
    let error = Config::from_source(&clash).expect_err("один порт на двоих");
    assert_eq!(error.keys(), [ADMIN_ENV]);
}

#[test]
fn config_debug_never_shows_the_private_key() {
    let dir = certs("user-debug");
    let config = Config {
        tls: Some(Tls::load(&dir, NAME).expect("сертификаты читаются")),
        ..Config::default()
    };
    let debug = format!("{config:?}");
    assert!(
        !debug.contains("PRIVATE KEY"),
        "ключ в логах это утечка: {debug}"
    );
    assert!(debug.contains("Secret(***)"), "{debug}");
}

struct Process {
    child: Child,
    lines: Receiver<String>,
    errors: Arc<Mutex<String>>,
}

impl Process {
    fn spawn(dir: &Path, vars: &[(&str, &str)]) -> Process {
        let mut child = Command::new(env!("CARGO_BIN_EXE_user-service"))
            .current_dir(dir)
            .env_clear()
            .envs(vars.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("бинарь user-service запускается");
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

    fn banner(&self) -> String {
        self.lines
            .recv_timeout(LIMIT)
            .unwrap_or_else(|_| panic!("нет строки о запуске, stderr: {}", self.stderr()))
    }

    fn stderr(&self) -> String {
        self.errors.lock().expect("stderr").clone()
    }

    fn signal(&self, name: &str) {
        let sent = Command::new("kill")
            .args([name, &self.child.id().to_string()])
            .status()
            .expect("kill");
        assert!(sent.success());
    }

    fn wait(&mut self) -> ExitStatus {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().expect("статус процесса") {
                std::thread::sleep(Duration::from_millis(50));
                return status;
            }
            assert!(
                started.elapsed() < LIMIT,
                "процесс не завершился за {LIMIT:?}, stderr: {}",
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

fn admin_of(banner: &str) -> String {
    let start = banner
        .find("admin http://")
        .unwrap_or_else(|| panic!("в строке запуска есть admin-адрес: {banner}"))
        + "admin http://".len();
    let rest = &banner[start..];
    rest[..rest.find('/').expect("адрес до /ready")].to_string()
}

async fn get(addr: &str, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.expect("admin-порт открыт");
    let ask = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(ask.as_bytes()).await.expect("запрос");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("ответ");
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

fn in_order(text: &str, needles: &[&str]) {
    let mut from = 0;
    for needle in needles {
        match text[from..].find(needle) {
            Some(at) => from += at + needle.len(),
            None => panic!("после позиции {from} нет «{needle}», порядок нарушен:\n{text}"),
        }
    }
}

#[tokio::test]
async fn the_binary_serves_health_and_ready_and_stops_in_order_on_sigterm() {
    let dir = scratch("user-binary");
    let certs = certs("user-binary-certs");
    let certs = certs.to_string_lossy();
    let mut process = Process::spawn(
        &dir,
        &[
            ("USERS_GRPC_ADDR", "127.0.0.1:0"),
            (ADMIN_ENV, "127.0.0.1:0"),
            ("CERTS_DIR", &certs),
            ("SHUTDOWN_GRACE_MS", "300"),
            ("ACCESS_LOG", "false"),
        ],
    );
    let banner = process.banner();
    let admin = admin_of(&banner);
    let (status, body) = get(&admin, "/health").await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = get(&admin, "/ready").await;
    assert_eq!(
        status, 200,
        "user-service готов, его журнал пишется: {body}"
    );
    assert!(body.contains("\"journal\""), "{body}");
    process.signal("-TERM");
    let status = process.wait();
    assert!(
        status.success(),
        "SIGTERM это штатная остановка с кодом 0: {status:?}, stderr: {}",
        process.stderr()
    );
    in_order(
        &process.stderr(),
        &[
            "получен SIGTERM",
            "не готов",
            "приём закрыт",
            "запросы в полёте завершились",
            "фоновые задачи остановлены",
            "остановлен",
        ],
    );
}

#[tokio::test]
async fn a_bad_config_fails_at_once_and_names_every_variable() {
    let dir = scratch("user-bad-config");
    let mut process = Process::spawn(
        &dir,
        &[
            ("USERS_GRPC_ADDR", "куда-нибудь"),
            ("SHUTDOWN_GRACE_MS", "скоро"),
            ("CERTS_DIR", "нет/такой/папки"),
        ],
    );
    let started = Instant::now();
    let status = process.wait();
    assert!(
        !status.success(),
        "кривой конфиг это падение, а не умолчание"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "кривой конфиг не лечится ожиданием: падаем сразу"
    );
    let stderr = process.stderr();
    assert!(
        stderr.contains("USERS_GRPC_ADDR") && stderr.contains("SHUTDOWN_GRACE_MS"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("сертификат"),
        "сначала конфиг, до файлов и портов: {stderr}"
    );
    assert!(process.lines.try_recv().is_err(), "порт так и не занят");
}

#[tokio::test]
async fn dotenv_in_the_working_directory_is_read_and_the_shell_wins() {
    let dir = scratch("user-dotenv");
    let certs = certs("user-dotenv-certs");
    std::fs::write(
        dir.join(".env"),
        format!(
            "USERS_GRPC_ADDR=127.0.0.1:0\nUSERS_ADMIN_ADDR=127.0.0.1:1\nCERTS_DIR={}\nACCESS_LOG=false\n",
            certs.display()
        ),
    )
    .expect(".env пишется");
    let mut process = Process::spawn(&dir, &[(ADMIN_ENV, "127.0.0.1:0")]);
    let banner = process.banner();
    let admin = admin_of(&banner);
    assert_ne!(admin, "127.0.0.1:1", "переменная окружения главнее .env");
    let (status, _) = get(&admin, "/health").await;
    assert_eq!(status, 200);
    process.signal("-INT");
    assert!(process.wait().success(), "Ctrl-C тоже штатная остановка");
    assert!(
        process.stderr().contains("получен SIGINT"),
        "{}",
        process.stderr()
    );
}
