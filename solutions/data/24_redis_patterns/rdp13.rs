use course_support::redis::{Client, Error};

async fn add(c: &Client, ns: &str, user: i64, points: i64) -> Result<(), Error> {
    c.cmd(&["ZINCRBY", &format!("{ns}:rating"), &points.to_string(), &user.to_string()]).await?;
    Ok(())
}

async fn screen(c: &Client, ns: &str, me: i64) -> Result<(Vec<i64>, Option<(i64, i64)>), Error> {
    let key = format!("{ns}:rating");
    let me = me.to_string();
    let replies = c.pipeline(&[&["ZRANGE", &key, "0", "9", "REV"], &["ZREVRANK", &key, &me], &["ZSCORE", &key, &me]]).await?;
    let top = replies.first().map(|v| v.texts()).unwrap_or_default().iter().filter_map(|t| t.parse().ok()).collect();
    let rank = replies.get(1).and_then(|v| v.int());
    let score = replies.get(2).and_then(|v| v.text()).and_then(|t| t.parse::<f64>().ok());
    Ok((top, rank.zip(score).map(|(r, s)| (r + 1, s as i64))))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn whole_screen_in_one_round_trip() {
        let s = Scope::new().await;
        let c = s.client();
        for u in 1..=15 {
            add(&c, s.prefix(), u, u * 10).await.expect("очки");
        }
        add(&c, s.prefix(), 3, 500).await.expect("очки");
        let before = c.round_trips();
        let (top, me) = screen(&c, s.prefix(), 7).await.expect("экран");
        assert_eq!(c.round_trips() - before, 1, "экран рейтинга это один обмен");
        assert_eq!(top, [3, 15, 14, 13, 12, 11, 10, 9, 8, 7], "топ-10 лучшими вперёд");
        assert_eq!(me, Some((10, 70)), "пользователь 7 на десятом месте с 70 очками");
        let (_, nobody) = screen(&c, s.prefix(), 99).await.expect("экран");
        assert_eq!(nobody, None, "кого нет в рейтинге, у того нет места");
    }
}
