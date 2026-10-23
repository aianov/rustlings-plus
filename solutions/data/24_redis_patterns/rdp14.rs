use course_support::redis::{Client, Error};

async fn fan_out(c: &Client, ns: &str, followers: &[i64], post: i64) -> Result<(), Error> {
    if followers.is_empty() {
        return Ok(());
    }
    let post = post.to_string();
    let keys: Vec<String> = followers.iter().map(|f| format!("{ns}:feed:{f}")).collect();
    let mut cmds: Vec<[&str; 4]> = Vec::with_capacity(keys.len() * 2);
    for key in &keys {
        cmds.push(["LPUSH", key, &post, ""]);
        cmds.push(["LTRIM", key, "0", "199"]);
    }
    let batch: Vec<&[&str]> = cmds.iter().map(|c| if c[0] == "LPUSH" { &c[..3] } else { &c[..] }).collect();
    c.pipeline(&batch).await?;
    Ok(())
}

async fn feed(c: &Client, ns: &str, user: i64, n: usize) -> Result<Vec<i64>, Error> {
    let stop = n.saturating_sub(1).to_string();
    Ok(c.cmd(&["LRANGE", &format!("{ns}:feed:{user}"), "0", &stop]).await?.texts().iter().filter_map(|t| t.parse().ok()).collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn fan_out_in_one_round_trip() {
        let s = Scope::new().await;
        let c = s.client();
        let followers: Vec<i64> = (1..=1000).collect();
        let before = c.round_trips();
        fan_out(&c, s.prefix(), &followers, 42).await.expect("раскладка");
        assert_eq!(c.round_trips() - before, 1, "раскладка на тысячу подписчиков это один обмен");
        assert_eq!(feed(&c, s.prefix(), 500, 10).await.expect("лента"), [42]);
        for post in 0..250 {
            fan_out(&c, s.prefix(), &[7], post).await.expect("раскладка");
        }
        let len = s.cmd(&["LLEN", &s.key("feed:7")]).await.expect("LLEN").int();
        assert_eq!(len, Some(200), "лента не длиннее 200");
        assert_eq!(feed(&c, s.prefix(), 7, 3).await.expect("лента"), [249, 248, 247], "свежие первыми");
    }
}
