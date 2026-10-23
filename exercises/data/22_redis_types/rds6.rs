// Профиль в кэше: имя, город и число подписчиков. Хранить его JSON-строкой неудобно: чтобы прибавить
// подписчика, надо прочитать, разобрать, поменять и записать, и параллельные изменения потеряются. Хеш
// Redis хранит поля по отдельности и умеет менять одно поле.
// TODO: `save` пишет все три поля одной командой, `load` читает профиль (None, если его нет),
// `add_follower` прибавляет подписчика и возвращает новое число.

use course_support::redis::{Client, Error};

#[derive(Debug, Clone, PartialEq)]
struct Profile {
    name: String,
    city: String,
    followers: i64,
}

async fn save(c: &Client, ns: &str, user: i64, p: &Profile) -> Result<(), Error> {
    let _ = (c, ns, user, p);
    todo!("HSET")
}

async fn load(c: &Client, ns: &str, user: i64) -> Result<Option<Profile>, Error> {
    let _ = (c, ns, user);
    todo!("HGETALL")
}

async fn add_follower(c: &Client, ns: &str, user: i64) -> Result<i64, Error> {
    let _ = (c, ns, user);
    todo!("HINCRBY")
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
