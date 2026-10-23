use course_support::redis::{Client, Error};

async fn record_view(c: &Client, ns: &str, profile: i64, viewer: i64) -> Result<(), Error> {
    let key = format!("{ns}:profile:{profile}:viewers");
    let viewer = viewer.to_string();
    c.pipeline(&[&["LREM", &key, "0", &viewer], &["LPUSH", &key, &viewer], &["LTRIM", &key, "0", "9"]]).await?;
    Ok(())
}

async fn recent(c: &Client, ns: &str, profile: i64) -> Result<Vec<String>, Error> {
    Ok(c.cmd(&["LRANGE", &format!("{ns}:profile:{profile}:viewers"), "0", "-1"]).await?.texts())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn last_ten_unique_viewers() {
        let s = Scope::new().await;
        let c = s.client();
        for v in 1..=12 {
            record_view(&c, s.prefix(), 7, v).await.expect("просмотр");
        }
        record_view(&c, s.prefix(), 7, 5).await.expect("повторный просмотр");
        let got = recent(&c, s.prefix(), 7).await.expect("список");
        assert_eq!(got, ["5", "12", "11", "10", "9", "8", "7", "6", "4", "3"], "десять последних, повторный гость переехал в начало");
    }
}
