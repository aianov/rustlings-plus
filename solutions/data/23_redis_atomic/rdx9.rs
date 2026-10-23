use course_support::redis::{Client, Error};

async fn post_event(c: &Client, stream: &str, kind: &str, data: &str) -> Result<String, Error> {
    c.cmd(&["XADD", stream, "MAXLEN", "~", "1000", "*", "kind", kind, "data", data])
        .await?
        .text()
        .ok_or_else(|| Error::Protocol("XADD без id".into()))
}

async fn read_after(addr: &str, stream: &str, last_id: &str, block_ms: u64) -> Result<Vec<(String, String)>, Error> {
    let c = Client::connect_to(addr).await?;
    let reply = c.cmd(&["XREAD", "COUNT", "10", "BLOCK", &block_ms.to_string(), "STREAMS", stream, last_id]).await?;
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
    Ok(out)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    use std::time::Duration;

    #[tokio::test]
    async fn stream_stays_bounded() {
        let s = Scope::new().await;
        let c = s.client();
        let stream = s.key("events");
        for i in 0..5000 {
            post_event(&c, &stream, "like", &i.to_string()).await.expect("XADD");
        }
        let len = s.cmd(&["XLEN", &stream]).await.expect("XLEN").int().unwrap_or(0);
        assert!((1000..=1200).contains(&len), "в стриме примерно последняя 1000 событий, а их {len}");
    }

    #[tokio::test]
    async fn blocked_reader_wakes_on_new_event() {
        let s = Scope::new().await;
        let stream = s.key("events");
        let last = post_event(&s.client(), &stream, "post", "старое").await.expect("XADD");
        let (addr, st, from) = (s.addr(), stream.clone(), last.clone());
        let reader = tokio::spawn(async move { read_after(&addr, &st, &from, 3000).await });
        tokio::time::sleep(Duration::from_millis(200)).await;
        post_event(&s.client(), &stream, "post", "новое").await.expect("XADD");
        let got = reader.await.expect("задача").expect("XREAD");
        assert_eq!(got.iter().map(|(_, d)| d.as_str()).collect::<Vec<_>>(), ["новое"], "читатель проснулся на новом событии и не получил старое");
    }
}
