use course_support::redis::{Client, Error};

async fn save_names(c: &Client, ns: &str, names: &[(i64, &str)]) -> Result<(), Error> {
    if names.is_empty() {
        return Ok(());
    }
    let keys: Vec<String> = names.iter().map(|(id, _)| format!("{ns}:user:{id}:name")).collect();
    let mut args = vec!["MSET"];
    for ((_, name), key) in names.iter().zip(&keys) {
        args.push(key);
        args.push(name);
    }
    c.cmd(&args).await?;
    Ok(())
}

async fn load_names(c: &Client, ns: &str, ids: &[i64]) -> Result<Vec<Option<String>>, Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<String> = ids.iter().map(|id| format!("{ns}:user:{id}:name")).collect();
    let mut args = vec!["MGET"];
    args.extend(keys.iter().map(String::as_str));
    Ok(c.cmd(&args).await?.list().iter().map(|v| v.text()).collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn one_round_trip_each_way() {
        let s = Scope::new().await;
        let c = s.client();
        let names: Vec<(i64, String)> = (1..=20).map(|i| (i, format!("user{i}"))).collect();
        let refs: Vec<(i64, &str)> = names.iter().map(|(i, n)| (*i, n.as_str())).collect();
        let before = c.round_trips();
        save_names(&c, s.prefix(), &refs).await.expect("запись");
        assert_eq!(c.round_trips() - before, 1, "двадцать имён записываются одним обменом");
        let ids: Vec<i64> = vec![3, 99, 20];
        let before = c.round_trips();
        let got = load_names(&c, s.prefix(), &ids).await.expect("чтение");
        assert_eq!(c.round_trips() - before, 1, "имена читаются одним обменом");
        assert_eq!(got, [Some("user3".to_string()), None, Some("user20".to_string())], "порядок как в запросе, нет имени это None на своём месте");
        assert!(load_names(&c, s.prefix(), &[]).await.expect("пусто").is_empty(), "пустой список это пустой ответ");
    }
}
