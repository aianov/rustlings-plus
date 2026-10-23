use course_support::redis::{Client, Error};

async fn claim(c: &Client, ns: &str, name: &str, user: i64) -> Result<bool, Error> {
    let key = format!("{ns}:username:{name}");
    let reply = c.cmd(&["SET", &key, &user.to_string(), "NX", "EX", "600"]).await?;
    Ok(!reply.is_nil())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn first_claim_wins_and_expires() {
        let s = Scope::new().await;
        let c = s.client();
        assert!(claim(&c, s.prefix(), "kate", 1).await.expect("бронь"), "свободный ник бронируется");
        assert!(!claim(&c, s.prefix(), "kate", 2).await.expect("бронь"), "занятый ник второму не достаётся");
        let key = s.key("username:kate");
        assert_eq!(s.cmd(&["GET", &key]).await.expect("GET").text().as_deref(), Some("1"), "бронь осталась у первого");
        let ttl = s.cmd(&["TTL", &key]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=600).contains(&ttl), "бронь живёт не больше 600 секунд, TTL = {ttl}");
    }
}
