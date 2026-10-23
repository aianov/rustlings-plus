// Лента показывает 20 постов, и для каждого нужно имя автора. Сейчас имена читаются по одному GET на автора:
// двадцать обменов с Redis на каждую ленту, и задержка ленты это двадцать задержек сети. Почини чтение и запись
// так, чтобы каждая из них была одним обменом. Отсутствующее имя это None на своём месте.

use course_support::redis::{Client, Error};

async fn save_names(c: &Client, ns: &str, names: &[(i64, &str)]) -> Result<(), Error> {
    for (id, name) in names {
        c.cmd(&["SET", &format!("{ns}:user:{id}:name"), name]).await?;
    }
    Ok(())
}

async fn load_names(c: &Client, ns: &str, ids: &[i64]) -> Result<Vec<Option<String>>, Error> {
    let mut out = Vec::new();
    for id in ids {
        out.push(c.cmd(&["GET", &format!("{ns}:user:{id}:name")]).await?.text());
    }
    Ok(out)
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
