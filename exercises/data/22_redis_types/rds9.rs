// Тренды: у каждого поста счёт популярности в sorted set, и блок «в тренде» показывает пять постов с самым
// высоким счётом, лучший первым, вместе со счётом. Сейчас блок показывает пять худших. Почини `top`, не
// сортируя ничего в сервисе: Redis хранит элементы упорядоченными по счёту, нужно только правильно попросить.

use course_support::redis::{Client, Error};

async fn set_score(c: &Client, ns: &str, post: i64, score: i64) -> Result<(), Error> {
    c.cmd(&["ZADD", &format!("{ns}:trending"), &score.to_string(), &post.to_string()]).await?;
    Ok(())
}

async fn top(c: &Client, ns: &str, n: usize) -> Result<Vec<(i64, i64)>, Error> {
    let stop = n.saturating_sub(1).to_string();
    let flat = c.cmd(&["ZRANGE", &format!("{ns}:trending"), "0", &stop, "WITHSCORES"]).await?.texts();
    Ok(flat.chunks(2).filter_map(|p| Some((p.first()?.parse().ok()?, p.get(1)?.parse().ok()?))).collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn best_five_first() {
        let s = Scope::new().await;
        let c = s.client();
        for (post, score) in [(1, 50), (2, 900), (3, 10), (4, 400), (5, 700), (6, 300), (7, 800), (8, 5)] {
            set_score(&c, s.prefix(), post, score).await.expect("ZADD");
        }
        set_score(&c, s.prefix(), 3, 1000).await.expect("ZADD обновляет счёт");
        assert_eq!(top(&c, s.prefix(), 5).await.expect("топ"), [(3, 1000), (2, 900), (7, 800), (5, 700), (4, 400)], "пять лучших, лучший первым");
    }
}
