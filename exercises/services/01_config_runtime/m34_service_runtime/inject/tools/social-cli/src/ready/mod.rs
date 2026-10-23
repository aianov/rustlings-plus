use std::io;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::{Instant, sleep};

use crate::http::Web;

pub const READY_PAUSE: Duration = Duration::from_millis(250);

pub const READY_LIMIT: Duration = Duration::from_secs(20);

enum Seen {
    Ready,
    Waiting(String),
    Silent(String),
}

fn unready(body: &str) -> String {
    let Ok(view) = serde_json::from_str::<Value>(body) else {
        return format!("непонятный ответ: {body}");
    };
    if view["stopping"].as_bool() == Some(true) {
        return "шлюз останавливается".to_string();
    }
    let failing: Vec<String> = view["checks"]
        .as_array()
        .map(|checks| {
            checks
                .iter()
                .filter(|check| check["ok"].as_bool() != Some(true))
                .map(|check| {
                    let name = check["name"].as_str().unwrap_or("?");
                    match check["reason"].as_str() {
                        Some(reason) => format!("{name} ({reason})"),
                        None => name.to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if failing.is_empty() {
        "причина не названа".to_string()
    } else {
        format!("не готов {}", failing.join(", "))
    }
}

async fn look(web: &mut Web) -> Seen {
    match web.request("GET", "/ready", None, None).await {
        Ok(reply) if reply.status == 200 => Seen::Ready,
        Ok(reply) => Seen::Waiting(unready(&reply.body)),
        Err(error) => Seen::Silent(error.to_string()),
    }
}

async fn say<W: AsyncWrite + Unpin>(out: &mut W, line: &str) -> io::Result<()> {
    out.write_all(line.as_bytes()).await?;
    out.write_all(b"\n").await?;
    out.flush().await
}

pub async fn await_ready<W>(admin: &str, limit: Duration, out: &mut W) -> io::Result<bool>
where
    W: AsyncWrite + Unpin,
{
    let mut web = Web::new(admin);
    let deadline = Instant::now() + limit;
    let mut last = String::new();
    loop {
        let line = match look(&mut web).await {
            Seen::Ready => {
                say(
                    out,
                    &format!("шлюз готов: http://{admin}/ready ответил 200"),
                )
                .await?;
                return Ok(true);
            }
            Seen::Waiting(why) => format!("жду шлюз: {why}"),
            Seen::Silent(error) => format!("жду шлюз: admin-порт http://{admin} молчит ({error})"),
        };
        if line != last {
            say(out, &line).await?;
            last = line;
        }
        if Instant::now() + READY_PAUSE > deadline {
            let gave_up = format!(
                "не дождался готовности за {} с, последнее: {}; продолжаю, команды к неготовым сервисам ответят ошибкой",
                limit.as_secs(),
                last.trim_start_matches("жду шлюз: ")
            );
            say(out, &gave_up).await?;
            return Ok(false);
        }
        sleep(READY_PAUSE).await;
    }
}
