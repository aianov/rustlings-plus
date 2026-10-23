use course_support::redis::{Client, Error};

#[derive(Debug, PartialEq)]
struct Card {
    views: i64,
    recent: Vec<String>,
    uniques: i64,
}

const MONTH: &str = "2592000";

async fn on_view(c: &Client, ns: &str, profile: i64, viewer: i64) -> Result<(), Error> {
    let hash = format!("{ns}:profile:{profile}");
    let recent = format!("{hash}:recent");
    let uniques = format!("{hash}:uniques");
    let v = viewer.to_string();
    c.pipeline(&[
        &["HINCRBY", &hash, "views", "1"],
        &["LREM", &recent, "0", &v],
        &["LPUSH", &recent, &v],
        &["LTRIM", &recent, "0", "4"],
        &["EXPIRE", &recent, MONTH],
        &["PFADD", &uniques, &v],
        &["EXPIRE", &uniques, MONTH],
    ])
    .await?;
    Ok(())
}

async fn card(c: &Client, ns: &str, profile: i64) -> Result<Card, Error> {
    let hash = format!("{ns}:profile:{profile}");
    let replies = c
        .pipeline(&[&["HGET", &hash, "views"], &["LRANGE", &format!("{hash}:recent"), "0", "-1"], &["PFCOUNT", &format!("{hash}:uniques")]])
        .await?;
    Ok(Card {
        views: replies.first().and_then(|v| v.int()).unwrap_or(0),
        recent: replies.get(1).map(|v| v.texts()).unwrap_or_default(),
        uniques: replies.get(2).and_then(|v| v.int()).unwrap_or(0),
    })
}

async fn tag_used(c: &Client, ns: &str, tag: &str) -> Result<(), Error> {
    c.cmd(&["ZINCRBY", &format!("{ns}:tags"), "1", tag]).await?;
    Ok(())
}

async fn top_tags(c: &Client, ns: &str, n: usize) -> Result<Vec<String>, Error> {
    let stop = n.saturating_sub(1).to_string();
    Ok(c.cmd(&["ZRANGE", &format!("{ns}:tags"), "0", &stop, "REV"]).await?.texts())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn profile_card() {
        let s = Scope::new().await;
        let c = s.client();
        let before = c.round_trips();
        on_view(&c, s.prefix(), 7, 1).await.expect("просмотр");
        assert_eq!(c.round_trips() - before, 1, "просмотр это один обмен");
        for v in [2, 3, 1, 4, 5, 6, 2] {
            on_view(&c, s.prefix(), 7, v).await.expect("просмотр");
        }
        let got = card(&c, s.prefix(), 7).await.expect("карточка");
        assert_eq!(got, Card { views: 8, recent: ["2", "6", "5", "4", "1"].map(String::from).to_vec(), uniques: 6 }, "8 просмотров, 5 последних, 6 уникальных");
        for key in [s.key("profile:7:recent"), s.key("profile:7:uniques")] {
            let ttl = s.cmd(&["TTL", &key]).await.expect("TTL").int().unwrap_or(-1);
            assert!((1..=2_592_000).contains(&ttl), "у {key} срок 30 дней, а TTL = {ttl}");
        }
        assert_eq!(card(&c, s.prefix(), 99).await.expect("пустая карточка"), Card { views: 0, recent: Vec::new(), uniques: 0 });
    }

    #[tokio::test]
    async fn top_tags_best_first() {
        let s = Scope::new().await;
        let c = s.client();
        for tag in ["rust", "go", "rust", "sql", "rust", "go", "zig"] {
            tag_used(&c, s.prefix(), tag).await.expect("тег");
        }
        assert_eq!(top_tags(&c, s.prefix(), 2).await.expect("топ"), ["rust", "go"], "два самых частых тега");
    }
}
