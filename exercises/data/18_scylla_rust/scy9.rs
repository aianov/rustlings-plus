// Профиль пользователя хранит разные типы: uuid, время, множество интересов, словарь ссылок и
// необязательный аватар. Драйвер умеет их все, нужно только подобрать Rust-типы. Тонкость Scylla: пустую
// коллекцию она хранит как отсутствие значения, и при чтении на её месте приходит NULL, а в профиле должно
// быть пустое множество.
// TODO: `save` записывает профиль, `load` читает его по id (None, если профиля нет).

use scylla::frame::value::CqlTimestamp;
use scylla::transport::errors::QueryError;
use scylla::Session;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use uuid::Uuid;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Clone, PartialEq)]
struct Profile {
    user_id: Uuid,
    created: CqlTimestamp,
    nickname: String,
    interests: HashSet<String>,
    links: HashMap<String, String>,
    avatar: Option<String>,
}

async fn save(session: &Session, p: &Profile) -> Result<(), QueryError> {
    let _ = (session, p);
    todo!("INSERT со всеми полями")
}

async fn load(session: &Session, id: Uuid) -> Result<Option<Profile>, BoxError> {
    let _ = (session, id);
    todo!("SELECT и разбор")
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::scylla::TestKs;

    const SCHEMA: &str = "CREATE TABLE profiles (user_id uuid PRIMARY KEY, created timestamp, nickname text, interests set<text>, links map<text, text>, avatar text)";

    fn full() -> Profile {
        Profile {
            user_id: Uuid::new_v4(),
            created: CqlTimestamp(1_792_000_000_123),
            nickname: "kate".to_string(),
            interests: ["rust", "scylla"].into_iter().map(String::from).collect(),
            links: [("github", "https://github.com/kate")].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            avatar: Some("kate.png".to_string()),
        }
    }

    #[tokio::test]
    async fn full_profile_round_trip() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let p = full();
        save(&ks.session, &p).await.expect("запись профиля");
        assert_eq!(load(&ks.session, p.user_id).await.expect("чтение профиля"), Some(p), "профиль читается таким, каким записан");
    }

    #[tokio::test]
    async fn empty_collections_and_no_avatar() {
        let ks = TestKs::with_cql(SCHEMA).await;
        let p = Profile { interests: HashSet::new(), links: HashMap::new(), avatar: None, ..full() };
        save(&ks.session, &p).await.expect("запись профиля");
        assert_eq!(load(&ks.session, p.user_id).await.expect("пустые коллекции не должны ломать чтение"), Some(p), "пустые коллекции читаются пустыми, а не ошибкой");
    }

    #[tokio::test]
    async fn unknown_profile_is_none() {
        let ks = TestKs::with_cql(SCHEMA).await;
        assert_eq!(load(&ks.session, Uuid::new_v4()).await.expect("чтение"), None, "нет профиля это None, а не ошибка");
    }
}
