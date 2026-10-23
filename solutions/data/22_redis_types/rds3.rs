use course_support::redis::{Client, Error};

async fn update_session(c: &Client, ns: &str, token: &str, data: &str) -> Result<(), Error> {
    c.cmd(&["SET", &format!("{ns}:session:{token}"), data, "KEEPTTL"]).await?;
    Ok(())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn update_keeps_the_expiry() {
        let s = Scope::new().await;
        let key = s.key("session:abc");
        s.cmd(&["SET", &key, "{\"theme\":\"light\"}", "EX", "1800"]).await.expect("сессия");
        update_session(&s.client(), s.prefix(), "abc", "{\"theme\":\"dark\"}").await.expect("обновление");
        assert_eq!(s.cmd(&["GET", &key]).await.expect("GET").text().as_deref(), Some("{\"theme\":\"dark\"}"), "данные обновились");
        let ttl = s.cmd(&["TTL", &key]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=1800).contains(&ttl), "срок сессии сохранился, а не исчез: TTL = {ttl}");
    }
}
