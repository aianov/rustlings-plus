// Мобильный клиент ждёт ключи в `camelCase` и ключ `type`, а на сервере поля названы как положено в Rust:
// `display_name`, `avatar_url`, `followers_count`, а `type` вообще зарезервированное слово, поэтому поле
// называется `kind`. Сейчас JSON получается с ключами `display_name` и клиент их не узнаёт. Ещё старые
// версии приложения до сих пор присылают имя под ключом `nick`: читать его нужно, а писать только `displayName`.
// TODO: Добейся нужного JSON, не переименовывая поля структуры.
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub followers_count: u32,
    pub kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ira() -> Profile {
        Profile { display_name: "Ira".into(), avatar_url: None, followers_count: 3, kind: "person".into() }
    }

    #[test]
    fn wire_names_are_camel_case() {
        assert_eq!(
            serde_json::to_string(&ira()).unwrap(),
            r#"{"displayName":"Ira","avatarUrl":null,"followersCount":3,"type":"person"}"#
        );
    }

    #[test]
    fn reads_the_new_format() {
        let raw = r#"{"displayName":"Ira","avatarUrl":"a.png","followersCount":3,"type":"person"}"#;
        let got: Profile = serde_json::from_str(raw).unwrap();
        assert_eq!(got.avatar_url.as_deref(), Some("a.png"));
        assert_eq!(got.kind, "person");
    }

    #[test]
    fn old_clients_send_nick() {
        let raw = r#"{"nick":"Ira","avatarUrl":null,"followersCount":3,"type":"person"}"#;
        let got: Profile = serde_json::from_str(raw).unwrap();
        assert_eq!(got, ira(), "ключ nick должен читаться как displayName");
        let again = serde_json::to_string(&got).unwrap();
        assert!(again.contains("\"displayName\":\"Ira\"") && !again.contains("nick"), "а писать нужно только новое имя: {again}");
    }

    #[test]
    fn rust_style_names_are_not_accepted() {
        let raw = r#"{"display_name":"Ira","avatar_url":null,"followers_count":3,"kind":"person"}"#;
        assert!(serde_json::from_str::<Profile>(raw).is_err(), "snake_case на проводе быть не должно");
    }
}
