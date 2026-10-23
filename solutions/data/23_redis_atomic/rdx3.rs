use course_support::redis::{Client, Error};
use std::time::Duration;

async fn buy(addr: &str, key: &str) -> Result<bool, Error> {
    let c = Client::connect_to(addr).await?;
    loop {
        c.cmd(&["WATCH", key]).await?;
        let left = c.cmd(&["GET", key]).await?.int().unwrap_or(0);
        if left <= 0 {
            c.cmd(&["UNWATCH"]).await?;
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        let replies = c.pipeline(&[&["MULTI"], &["DECR", key], &["EXEC"]]).await?;
        if replies.last().is_some_and(|v| !v.is_nil()) {
            return Ok(true);
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn never_oversold() {
        let s = Scope::new().await;
        let key = s.key("tickets:concert");
        s.cmd(&["SET", &key, "10"]).await.expect("остаток");
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let (addr, key) = (s.addr(), key.clone());
            tasks.push(tokio::spawn(async move { buy(&addr, &key).await.expect("покупка") }));
        }
        let mut sold = 0;
        for t in tasks {
            if t.await.expect("задача") {
                sold += 1;
            }
        }
        assert_eq!(sold, 10, "продано ровно 10 билетов из 10");
        assert_eq!(s.cmd(&["GET", &key]).await.expect("GET").int(), Some(0), "остаток ноль, а не меньше");
    }
}
