use course_support::redis::{Client, Error};
use std::collections::BTreeSet;

async fn session_keys(c: &Client, ns: &str, user: i64) -> Result<Vec<String>, Error> {
    let pattern = format!("{ns}:session:{user}:*");
    let mut cursor = "0".to_string();
    let mut found = BTreeSet::new();
    loop {
        let reply = c.cmd(&["SCAN", &cursor, "MATCH", &pattern, "COUNT", "100"]).await?.list();
        cursor = reply.first().and_then(|v| v.text()).ok_or_else(|| Error::Protocol("SCAN без курсора".into()))?;
        found.extend(reply.get(1).map(|v| v.texts()).unwrap_or_default());
        if cursor == "0" {
            break;
        }
    }
    Ok(found.into_iter().collect())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    const SOURCE: &str = include_str!("rds12.rs");

    #[tokio::test]
    async fn finds_every_session_once() {
        let s = Scope::new().await;
        let c = s.client();
        let mut cmds: Vec<Vec<String>> = Vec::new();
        for i in 0..600 {
            let user = if i % 50 == 0 { 7 } else { 1000 + i };
            cmds.push(vec!["SET".into(), s.key(&format!("session:{user}:{i}")), "x".into()]);
        }
        let refs: Vec<Vec<&str>> = cmds.iter().map(|c| c.iter().map(String::as_str).collect()).collect();
        let batch: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
        c.pipeline(&batch).await.expect("загрузка");
        let got = session_keys(&c, s.prefix(), 7).await.expect("поиск");
        let mut want: Vec<String> = (0..600).filter(|i| i % 50 == 0).map(|i| s.key(&format!("session:7:{i}"))).collect();
        want.sort();
        assert_eq!(got, want, "двенадцать сессий пользователя 7, каждая один раз");
    }

    #[test]
    fn no_keys_command() {
        let code = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(!code.contains("\"KEYS\""), "KEYS блокирует сервер, обходи ключи через SCAN");
    }
}
