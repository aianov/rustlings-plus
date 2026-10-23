use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(alias = "nick")]
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub followers_count: u32,
    #[serde(rename = "type")]
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
