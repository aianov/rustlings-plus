use service_runtime::config::{ConfigError, Secret, layered, parse_dotenv, parse_or, required};
use service_runtime::health::Health;
use service_runtime::lifecycle::Lifecycle;
use service_runtime::signals::ShutdownListener;
use service_runtime::startup::{Policy, wait_for};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug)]
pub struct Settings {
    pub port: u16,
    pub grace: Duration,
    pub drain_deadline: Duration,
    pub database_addr: String,
    pub api_token: Secret<String>,
}

impl Settings {
    pub fn from_source(get: impl Fn(&str) -> Option<String>) -> Result<Settings, Vec<ConfigError>> {
        let port = parse_or("PORT", get("PORT"), 7000u16);
        let grace = parse_or("SHUTDOWN_GRACE_MS", get("SHUTDOWN_GRACE_MS"), 100u64);
        let drain = parse_or("DRAIN_DEADLINE_MS", get("DRAIN_DEADLINE_MS"), 2000u64);
        let database = required("DATABASE_ADDR", get("DATABASE_ADDR"));
        let token = required("API_TOKEN", get("API_TOKEN"));
        let port = match port {
            Ok(0) => Err(ConfigError::Invalid { name: "PORT".to_string(), value: "0".to_string() }),
            other => other,
        };
        match (port, grace, drain, database, token) {
            (Ok(port), Ok(grace), Ok(drain), Ok(database_addr), Ok(token)) => Ok(Settings {
                port,
                grace: Duration::from_millis(grace),
                drain_deadline: Duration::from_millis(drain),
                database_addr,
                api_token: Secret::new(token),
            }),
            (port, grace, drain, database, token) => Err([
                port.err(),
                grace.err(),
                drain.err(),
                database.err(),
                token.err(),
            ]
            .into_iter()
            .flatten()
            .collect()),
        }
    }

    pub fn load(
        env_file: &str,
        real: impl Fn(&str) -> Option<String>,
    ) -> Result<Settings, Vec<ConfigError>> {
        let file = parse_dotenv(env_file).map_err(|e| {
            vec![ConfigError::Invalid { name: ".env".to_string(), value: format!("строка {}", e.line) }]
        })?;
        Settings::from_source(layered(&file, real))
    }
}

async fn answer(mut stream: TcpStream, health: Arc<Health>) {
    let mut buf = [0u8; 512];
    let Ok(n) = stream.read(&mut buf).await else {
        return;
    };
    let head = String::from_utf8_lossy(&buf[..n]);
    let status = match head.split_whitespace().nth(1) {
        Some("/health") if health.live() => 200,
        Some("/ready") if health.ready().await.ready => 200,
        Some("/health" | "/ready") => 503,
        _ => 404,
    };
    let reply = format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(reply.as_bytes()).await;
}

pub async fn run() -> Result<(), String> {
    let file = std::fs::read_to_string(".env").unwrap_or_default();
    let settings = Settings::load(&file, |name| std::env::var(name).ok()).map_err(|errors| {
        errors.iter().map(|e| format!("* {e}")).collect::<Vec<_>>().join("\n")
    })?;
    let mut signals = ShutdownListener::new().map_err(|e| e.to_string())?;
    let policy = Policy { interval: Duration::from_millis(500), total: Duration::from_secs(30) };
    wait_for("database", &policy, || async {
        TcpStream::connect(&settings.database_addr).await.map(|_| ()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("{e:?}"))?;
    let listener = TcpListener::bind(("0.0.0.0", settings.port)).await.map_err(|e| e.to_string())?;
    let health = Arc::new(Health::new(Duration::from_secs(1)));
    let accept_health = Arc::clone(&health);
    let accept = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(answer(stream, Arc::clone(&accept_health)));
        }
    });
    let lifecycle = Lifecycle::new(vec![accept]);
    let signal = signals.wait().await;
    health.begin_shutdown();
    println!("получен {signal:?}, останавливаюсь");
    lifecycle.shutdown(settings.grace, settings.drain_deadline).await;
    Ok(())
}
