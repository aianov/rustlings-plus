// Сессия живёт 30 минут с последней активности: каждый запрос с токеном должен продлевать её. Сейчас
// `touch` только читает пользователя, и сессия истекает через полчаса после входа, даже если пользователь
// активен. Почини `touch`: прочитать и продлить одним шагом.

use course_support::redis::{Client, Error};
use uuid::Uuid;

async fn login(c: &Client, ns: &str, user: i64) -> Result<String, Error> {
    let token = Uuid::new_v4().to_string();
    c.cmd(&["SET", &format!("{ns}:session:{token}"), &user.to_string(), "EX", "1800"]).await?;
    Ok(token)
}

async fn touch(c: &Client, ns: &str, token: &str) -> Result<Option<i64>, Error> {
    Ok(c.cmd(&["GET", &format!("{ns}:session:{token}")]).await?.int())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn activity_extends_the_session() {
        let s = Scope::new().await;
        let c = s.client();
        let token = login(&c, s.prefix(), 7).await.expect("вход");
        let key = s.key(&format!("session:{token}"));
        s.cmd(&["EXPIRE", &key, "10"]).await.expect("почти истекла");
        assert_eq!(touch(&c, s.prefix(), &token).await.expect("запрос"), Some(7), "сессия пользователя 7");
        let ttl = s.cmd(&["TTL", &key]).await.expect("TTL").int().unwrap_or(-1);
        assert!(ttl > 1700, "активность продлила сессию до 30 минут, TTL = {ttl}");
        assert_eq!(touch(&c, s.prefix(), "нет-такого").await.expect("запрос"), None, "чужой токен это None");
    }
}
