// Воркер взял три уведомления из группы и упал, не подтвердив их. Они навсегда висят в его списке ожидающих:
// `XREADGROUP ... >` их не отдаёт, ведь они уже были выданы. Второй воркер должен забирать себе сообщения,
// которые чужой воркер держит дольше заданного срока, и обрабатывать их. Почини `reclaim`.

use course_support::redis::{Client, Error};

async fn reclaim(c: &Client, stream: &str, group: &str, consumer: &str, min_idle_ms: u64) -> Result<Vec<String>, Error> {
    let _ = min_idle_ms;
    let reply = c.cmd(&["XREADGROUP", "GROUP", group, consumer, "COUNT", "100", "STREAMS", stream, ">"]).await?;
    let mut out = Vec::new();
    for s in reply.list() {
        for entry in s.list().get(1).map(|v| v.list()).unwrap_or_default() {
            out.push(entry.list().get(1).map(|v| v.texts()).unwrap_or_default().get(1).cloned().unwrap_or_default());
        }
    }
    Ok(out)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;

    #[tokio::test]
    async fn stuck_messages_are_taken_over() {
        let s = Scope::new().await;
        let c = s.client();
        let stream = s.key("notifications");
        c.cmd(&["XGROUP", "CREATE", &stream, "senders", "$", "MKSTREAM"]).await.expect("группа");
        for i in 0..3 {
            c.cmd(&["XADD", &stream, "*", "data", &format!("уведомление {i}")]).await.expect("XADD");
        }
        c.cmd(&["XREADGROUP", "GROUP", "senders", "worker-a", "COUNT", "10", "STREAMS", &stream, ">"]).await.expect("воркер a взял и упал");
        assert!(reclaim(&c, &stream, "senders", "worker-b", 10_000).await.expect("рано").is_empty(), "свежие чужие сообщения не трогаем");
        tokio::time::sleep(Duration::from_millis(150)).await;
        let got = reclaim(&c, &stream, "senders", "worker-b", 100).await.expect("перехват");
        assert_eq!(got, ["уведомление 0", "уведомление 1", "уведомление 2"], "зависшие у упавшего воркера сообщения перешли ко второму");
        let pending = c.cmd(&["XPENDING", &stream, "senders", "-", "+", "10", "worker-b"]).await.expect("XPENDING").list();
        assert_eq!(pending.len(), 3, "теперь они числятся за вторым воркером");
    }
}
