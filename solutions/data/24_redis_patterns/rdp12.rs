use course_support::redis::{Client, Error};

async fn heartbeat(c: &Client, ns: &str, user: i64) -> Result<(), Error> {
    c.cmd(&["SET", &format!("{ns}:presence:{user}"), "1", "EX", "30"]).await?;
    Ok(())
}

async fn online_friends(c: &Client, ns: &str, friends: &[i64]) -> Result<Vec<i64>, Error> {
    if friends.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<String> = friends.iter().map(|f| format!("{ns}:presence:{f}")).collect();
    let mut args = vec!["MGET"];
    args.extend(keys.iter().map(String::as_str));
    let values = c.cmd(&args).await?.list();
    Ok(friends.iter().zip(values).filter(|(_, v)| !v.is_nil()).map(|(f, _)| *f).collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn one_round_trip_for_all_friends() {
        let s = Scope::new().await;
        let c = s.client();
        for u in [3, 50, 120, 299] {
            heartbeat(&c, s.prefix(), u).await.expect("heartbeat");
        }
        let friends: Vec<i64> = (1..=300).collect();
        let before = c.round_trips();
        let online = online_friends(&c, s.prefix(), &friends).await.expect("онлайн");
        assert_eq!(c.round_trips() - before, 1, "300 друзей проверяются одним обменом");
        assert_eq!(online, [3, 50, 120, 299], "в сети четверо, в порядке списка друзей");
        assert!(online_friends(&c, s.prefix(), &[]).await.expect("пусто").is_empty());
    }
}
