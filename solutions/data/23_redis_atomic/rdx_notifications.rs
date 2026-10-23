use course_support::redis::{Client, Error, Value};

const NOTIFY: &str = "
redis.call('XADD', KEYS[1], 'MAXLEN', '~', '1000', '*', 'data', ARGV[1])
return redis.call('INCR', KEYS[2])
";

fn records(list: Vec<Value>) -> Vec<(String, String)> {
    list.iter()
        .map(|e| {
            let parts = e.list();
            let id = parts.first().and_then(|v| v.text()).unwrap_or_default();
            let text = parts.get(1).map(|f| f.texts()).and_then(|f| f.get(1).cloned()).unwrap_or_default();
            (id, text)
        })
        .collect()
}

async fn notify(c: &Client, stream: &str, unread_key: &str, text: &str) -> Result<i64, Error> {
    c.cmd(&["EVAL", NOTIFY, "2", stream, unread_key, text]).await?.int().ok_or_else(|| Error::Protocol("скрипт вернул не число".into()))
}

async fn start(c: &Client, stream: &str, group: &str) -> Result<(), Error> {
    match c.cmd(&["XGROUP", "CREATE", stream, group, "$", "MKSTREAM"]).await {
        Err(Error::Server(e)) if e.starts_with("BUSYGROUP") => Ok(()),
        other => other.map(|_| ()),
    }
}

async fn take(c: &Client, stream: &str, group: &str, consumer: &str, n: usize) -> Result<Vec<(String, String)>, Error> {
    let reply = c.cmd(&["XREADGROUP", "GROUP", group, consumer, "COUNT", &n.to_string(), "STREAMS", stream, ">"]).await?;
    Ok(reply.list().iter().flat_map(|s| records(s.list().get(1).map(|v| v.list()).unwrap_or_default())).collect())
}

async fn done(c: &Client, stream: &str, group: &str, ids: &[String]) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut args = vec!["XACK", stream, group];
    args.extend(ids.iter().map(String::as_str));
    c.cmd(&args).await?;
    Ok(())
}

async fn rescue(c: &Client, stream: &str, group: &str, consumer: &str, idle_ms: u64) -> Result<Vec<String>, Error> {
    let reply = c.cmd(&["XAUTOCLAIM", stream, group, consumer, &idle_ms.to_string(), "0-0", "COUNT", "100"]).await?;
    Ok(records(reply.list().get(1).map(|v| v.list()).unwrap_or_default()).into_iter().map(|(_, t)| t).collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;

    #[tokio::test]
    async fn notification_queue() {
        let s = Scope::new().await;
        let c = s.client();
        let (stream, unread) = (s.key("notify"), s.key("user:7:unread"));
        start(&c, &stream, "push").await.expect("группа");
        start(&c, &stream, "push").await.expect("повторный старт");
        for (i, text) in ["лайк", "подписка", "ответ"].iter().enumerate() {
            assert_eq!(notify(&c, &stream, &unread, text).await.expect("уведомление"), i as i64 + 1, "счётчик растёт вместе с записью");
        }
        let a = take(&c, &stream, "push", "a", 2).await.expect("воркер a");
        assert_eq!(a.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["лайк", "подписка"]);
        done(&c, &stream, "push", &[a[0].0.clone()]).await.expect("подтверждение");
        let b = take(&c, &stream, "push", "b", 5).await.expect("воркер b");
        assert_eq!(b.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["ответ"]);
        tokio::time::sleep(Duration::from_millis(150)).await;
        let rescued = rescue(&c, &stream, "push", "c", 100).await.expect("перехват");
        assert_eq!(rescued.len(), 2, "у a зависла неподтверждённая «подписка», у b «ответ»: {rescued:?}");
        assert!(rescued.contains(&"подписка".to_string()) && rescued.contains(&"ответ".to_string()));
    }

    #[tokio::test]
    async fn stream_is_bounded_and_atomic() {
        let s = Scope::new().await;
        let c = s.client();
        let (stream, unread) = (s.key("notify"), s.key("user:7:unread"));
        for i in 0..3000 {
            notify(&c, &stream, &unread, &format!("n{i}")).await.expect("уведомление");
        }
        let len = s.cmd(&["XLEN", &stream]).await.expect("XLEN").int().unwrap_or(0);
        assert!((1000..=1200).contains(&len), "стрим держит примерно 1000, а не {len}");
        assert_eq!(s.cmd(&["GET", &unread]).await.expect("GET").int(), Some(3000), "счётчик учёл каждое уведомление");
    }
}
