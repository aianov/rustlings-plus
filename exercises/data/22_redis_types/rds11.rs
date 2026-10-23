// Уникальные просмотры поста считаются множеством id зрителей. Цифра точная, но у популярных постов
// множество разрастается до мегабайт на пост, и Redis съедает всю память. Точная цифра не нужна: экрану
// достаточно «≈ 20 тыс.» с погрешностью в пару процентов. В Redis есть структура для приблизительного
// подсчёта уникальных, которая занимает не больше 12 КБ на ключ при любом числе зрителей. Перейди на неё.

use course_support::redis::{Client, Error};

async fn record(c: &Client, ns: &str, post: i64, viewers: &[i64]) -> Result<(), Error> {
    let key = format!("{ns}:post:{post}:uniques");
    let ids: Vec<String> = viewers.iter().map(|v| v.to_string()).collect();
    let mut args = vec!["SADD", key.as_str()];
    args.extend(ids.iter().map(String::as_str));
    c.cmd(&args).await?;
    Ok(())
}

async fn unique_views(c: &Client, ns: &str, post: i64) -> Result<i64, Error> {
    Ok(c.cmd(&["SCARD", &format!("{ns}:post:{post}:uniques")]).await?.int().unwrap_or(0))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn approximate_and_small() {
        let s = Scope::new().await;
        let c = s.client();
        for chunk in 0..20 {
            let viewers: Vec<i64> = (chunk * 1000..chunk * 1000 + 1000).collect();
            record(&c, s.prefix(), 7, &viewers).await.expect("просмотры");
        }
        record(&c, s.prefix(), 7, &(0..5000).collect::<Vec<i64>>()).await.expect("повторные просмотры");
        let n = unique_views(&c, s.prefix(), 7).await.expect("подсчёт");
        assert!((19_600..=20_400).contains(&n), "20000 уникальных с погрешностью в 2%, а получилось {n}");
        let bytes = s.cmd(&["MEMORY", "USAGE", &s.key("post:7:uniques")]).await.expect("MEMORY USAGE").int().unwrap_or(i64::MAX);
        assert!(bytes <= 16_000, "ключ должен занимать не больше 16 КБ, а занимает {bytes} байт");
    }
}
