use course_support::redis::{Client, Error};
use uuid::Uuid;

async fn login(c: &Client, ns: &str, user: i64) -> Result<String, Error> {
    let token = Uuid::new_v4().to_string();
    let session = format!("{ns}:session:{token}");
    let set = format!("{ns}:user:{user}:sessions");
    c.pipeline(&[&["SET", &session, &user.to_string(), "EX", "1800"], &["SADD", &set, &token], &["EXPIRE", &set, "1800"]]).await?;
    Ok(token)
}

async fn user_of(c: &Client, ns: &str, token: &str) -> Result<Option<i64>, Error> {
    Ok(c.cmd(&["GET", &format!("{ns}:session:{token}")]).await?.int())
}

async fn logout_all(c: &Client, ns: &str, user: i64) -> Result<usize, Error> {
    let set = format!("{ns}:user:{user}:sessions");
    let tokens = c.cmd(&["SMEMBERS", &set]).await?.texts();
    let keys: Vec<String> = tokens.iter().map(|t| format!("{ns}:session:{t}")).collect();
    let mut args = vec!["DEL", set.as_str()];
    args.extend(keys.iter().map(String::as_str));
    c.cmd(&args).await?;
    Ok(tokens.len())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn every_device_is_logged_out() {
        let s = Scope::new().await;
        let c = s.client();
        let before = c.round_trips();
        let phone = login(&c, s.prefix(), 7).await.expect("телефон");
        assert_eq!(c.round_trips() - before, 1, "вход это один обмен");
        let laptop = login(&c, s.prefix(), 7).await.expect("ноутбук");
        let tablet = login(&c, s.prefix(), 7).await.expect("планшет");
        let other = login(&c, s.prefix(), 9).await.expect("другой пользователь");
        assert_eq!(user_of(&c, s.prefix(), &laptop).await.expect("сессия"), Some(7));
        assert_eq!(logout_all(&c, s.prefix(), 7).await.expect("выход везде"), 3, "закрыто три сессии");
        for t in [&phone, &laptop, &tablet] {
            assert_eq!(user_of(&c, s.prefix(), t).await.expect("сессия"), None, "сессия {t} закрыта");
        }
        assert_eq!(user_of(&c, s.prefix(), &other).await.expect("сессия"), Some(9), "чужие сессии живы");
        assert!(s.cmd(&["EXISTS", &s.key("user:7:sessions")]).await.expect("EXISTS").int() == Some(0), "множество сессий удалено");
    }
}
