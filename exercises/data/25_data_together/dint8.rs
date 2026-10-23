// Профиль в кэше Redis обновляется событиями «профиль изменён» с номером версии. События приходят
// повторно и не по порядку, и кэш не должен откатиться на старую версию. Храни профиль хешем с полями version и
// data и обновляй его скриптом Lua только если версия события больше версии в кэше. Срок кэша 1 час.
// TODO: `on_profile_changed` (true, если кэш обновлён) и `cached` (данные из кэша или None).

use course_support::redis::{Client, Error};

async fn on_profile_changed(c: &Client, ns: &str, user: i64, version: i64, data: &str) -> Result<bool, Error> {
    let _ = (c, ns, user, version, data);
    todo!("скрипт с проверкой версии")
}

async fn cached(c: &Client, ns: &str, user: i64) -> Result<Option<String>, Error> {
    let _ = (c, ns, user);
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn cache_never_goes_back() {
        let s = Scope::new().await;
        let c = s.client();
        assert!(on_profile_changed(&c, s.prefix(), 7, 2, "{\"name\":\"kate v2\"}").await.expect("v2"));
        assert!(!on_profile_changed(&c, s.prefix(), 7, 1, "{\"name\":\"kate v1\"}").await.expect("v1 опоздала"), "старая версия не применяется");
        assert!(!on_profile_changed(&c, s.prefix(), 7, 2, "{\"name\":\"kate v2\"}").await.expect("повтор v2"), "повтор не нужен");
        assert_eq!(cached(&c, s.prefix(), 7).await.expect("кэш").as_deref(), Some("{\"name\":\"kate v2\"}"));
        assert!(on_profile_changed(&c, s.prefix(), 7, 3, "{\"name\":\"kate v3\"}").await.expect("v3"));
        assert_eq!(cached(&c, s.prefix(), 7).await.expect("кэш").as_deref(), Some("{\"name\":\"kate v3\"}"));
        let ttl = s.cmd(&["TTL", &s.key("profile:7")]).await.expect("TTL").int().unwrap_or(-1);
        assert!((1..=3600).contains(&ttl), "срок кэша час, TTL = {ttl}");
        assert_eq!(cached(&c, s.prefix(), 8).await.expect("кэш"), None);
    }
}
