// Счётчик просмотров поста читается, увеличивается в сервисе и записывается обратно. При нагрузке цифры
// меньше реальных: два запроса читают одно число, оба прибавляют единицу и оба пишут одно и то же. Почини
// так, чтобы ни один просмотр не терялся при любом числе одновременных запросов, а функция возвращала
// номер этого просмотра.

use course_support::redis::{Client, Error};
use std::time::Duration;

async fn view(c: &Client, ns: &str, post: i64) -> Result<i64, Error> {
    let key = format!("{ns}:post:{post}:views");
    let now = c.cmd(&["GET", &key]).await?.int().unwrap_or(0);
    tokio::time::sleep(Duration::from_millis(5)).await;
    c.cmd(&["SET", &key, &(now + 1).to_string()]).await?;
    Ok(now + 1)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn no_view_is_lost() {
        let s = Scope::new().await;
        let mut tasks = Vec::new();
        for _ in 0..50 {
            let c = s.client();
            let ns = s.prefix().to_string();
            tasks.push(tokio::spawn(async move { view(&c, &ns, 7).await.expect("просмотр") }));
        }
        let mut numbers = Vec::new();
        for t in tasks {
            numbers.push(t.await.expect("задача"));
        }
        numbers.sort_unstable();
        assert_eq!(numbers, (1..=50).collect::<Vec<i64>>(), "каждый из 50 просмотров получил свой номер");
        assert_eq!(s.cmd(&["GET", &s.key("post:7:views")]).await.expect("GET").int(), Some(50), "в счётчике ровно 50");
    }
}
