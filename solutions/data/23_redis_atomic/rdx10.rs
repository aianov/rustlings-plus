use course_support::redis::{Client, Error, Value};

async fn ensure_group(c: &Client, stream: &str, group: &str) -> Result<(), Error> {
    match c.cmd(&["XGROUP", "CREATE", stream, group, "$", "MKSTREAM"]).await {
        Err(Error::Server(e)) if e.starts_with("BUSYGROUP") => Ok(()),
        other => other.map(|_| ()),
    }
}

fn entries(reply: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for s in reply.list() {
        for entry in s.list().get(1).map(|v| v.list()).unwrap_or_default() {
            let parts = entry.list();
            let id = parts.first().and_then(|v| v.text()).unwrap_or_default();
            let fields = parts.get(1).map(|v| v.texts()).unwrap_or_default();
            let data = fields.chunks(2).find(|p| p.first().map(String::as_str) == Some("data")).and_then(|p| p.get(1).cloned()).unwrap_or_default();
            out.push((id, data));
        }
    }
    out
}

async fn consume(c: &Client, stream: &str, group: &str, consumer: &str, count: usize) -> Result<Vec<(String, String)>, Error> {
    let reply = c.cmd(&["XREADGROUP", "GROUP", group, consumer, "COUNT", &count.to_string(), "STREAMS", stream, ">"]).await?;
    Ok(entries(&reply))
}

async fn ack(c: &Client, stream: &str, group: &str, ids: &[String]) -> Result<i64, Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut args = vec!["XACK", stream, group];
    args.extend(ids.iter().map(String::as_str));
    c.cmd(&args).await?.int().ok_or_else(|| Error::Protocol("XACK вернул не число".into()))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn group_splits_and_acks() {
        let s = Scope::new().await;
        let c = s.client();
        let stream = s.key("notifications");
        ensure_group(&c, &stream, "senders").await.expect("группа на пустом месте");
        ensure_group(&c, &stream, "senders").await.expect("повторное создание группы не ошибка");
        for i in 0..6 {
            c.cmd(&["XADD", &stream, "*", "data", &format!("уведомление {i}")]).await.expect("XADD");
        }
        let a = consume(&c, &stream, "senders", "worker-a", 4).await.expect("воркер a");
        let b = consume(&c, &stream, "senders", "worker-b", 4).await.expect("воркер b");
        assert_eq!(a.len(), 4, "первый воркер взял четыре");
        assert_eq!(b.len(), 2, "второму достались оставшиеся два");
        let mut all: Vec<String> = a.iter().chain(&b).map(|(_, d)| d.clone()).collect();
        all.sort();
        assert_eq!(all, (0..6).map(|i| format!("уведомление {i}")).collect::<Vec<_>>(), "каждое уведомление ровно одному воркеру");
        let pending = c.cmd(&["XPENDING", &stream, "senders"]).await.expect("XPENDING").list();
        assert_eq!(pending.first().and_then(|v| v.int()), Some(6), "до подтверждения все шесть ждут");
        let ids: Vec<String> = a.iter().chain(&b).map(|(id, _)| id.clone()).collect();
        assert_eq!(ack(&c, &stream, "senders", &ids).await.expect("XACK"), 6);
        let pending = c.cmd(&["XPENDING", &stream, "senders"]).await.expect("XPENDING").list();
        assert_eq!(pending.first().and_then(|v| v.int()), Some(0), "после подтверждения ожидающих нет");
        assert!(consume(&c, &stream, "senders", "worker-a", 4).await.expect("пусто").is_empty(), "новых сообщений нет");
    }
}
