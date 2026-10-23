use course_support::redis::{Client, Error};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
struct Profile {
    name: String,
    city: String,
    followers: i64,
}

fn key(ns: &str, user: i64) -> String {
    format!("{ns}:user:{user}:profile")
}

async fn save(c: &Client, ns: &str, user: i64, p: &Profile) -> Result<(), Error> {
    let followers = p.followers.to_string();
    c.cmd(&["HSET", &key(ns, user), "name", &p.name, "city", &p.city, "followers", &followers]).await?;
    Ok(())
}

async fn load(c: &Client, ns: &str, user: i64) -> Result<Option<Profile>, Error> {
    let flat = c.cmd(&["HGETALL", &key(ns, user)]).await?.texts();
    if flat.is_empty() {
        return Ok(None);
    }
    let fields: HashMap<&str, &str> = flat.chunks(2).filter_map(|p| Some((p.first()?.as_str(), p.get(1)?.as_str()))).collect();
    let get = |f: &str| fields.get(f).map(|v| v.to_string()).unwrap_or_default();
    Ok(Some(Profile { name: get("name"), city: get("city"), followers: get("followers").parse().unwrap_or(0) }))
}

async fn add_follower(c: &Client, ns: &str, user: i64) -> Result<i64, Error> {
    c.cmd(&["HINCRBY", &key(ns, user), "followers", "1"]).await?.int().ok_or_else(|| Error::Protocol("HINCRBY вернул не число".into()))
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::redis::Scope;

    #[tokio::test]
    async fn profile_in_a_hash() {
        let s = Scope::new().await;
        let c = s.client();
        let p = Profile { name: "kate".to_string(), city: "Казань".to_string(), followers: 10 };
        let before = c.round_trips();
        save(&c, s.prefix(), 7, &p).await.expect("запись");
        assert_eq!(c.round_trips() - before, 1, "профиль пишется одной командой");
        assert_eq!(load(&c, s.prefix(), 7).await.expect("чтение"), Some(p.clone()));
        assert_eq!(load(&c, s.prefix(), 8).await.expect("чтение"), None, "нет профиля это None");
        assert_eq!(add_follower(&c, s.prefix(), 7).await.expect("подписчик"), 11);
        let after = load(&c, s.prefix(), 7).await.expect("чтение").expect("профиль");
        assert_eq!(after, Profile { followers: 11, ..p }, "меняется только число подписчиков");
        assert_eq!(s.cmd(&["TYPE", &s.key("user:7:profile")]).await.expect("TYPE").text().as_deref(), Some("hash"), "профиль это хеш");
    }
}
