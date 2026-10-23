// Кэш ленты: при публикации id поста кладётся в начало ленты каждого подписчика, лента хранит 200 последних
// id. Сейчас на каждого подписчика две отдельные команды, и пост автора с тысячей подписчиков это две тысячи
// обменов с Redis. Почини `fan_out` так, чтобы вся раскладка шла одним обменом, а лента не росла больше 200.

use course_support::redis::{Client, Error};

async fn fan_out(c: &Client, ns: &str, followers: &[i64], post: i64) -> Result<(), Error> {
    for f in followers {
        let key = format!("{ns}:feed:{f}");
        c.cmd(&["LPUSH", &key, &post.to_string()]).await?;
        c.cmd(&["LTRIM", &key, "0", "199"]).await?;
    }
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
