use serde::{Deserialize, Serialize};

fn default_theme() -> String {
    "light".to_string()
}

fn default_page_size() -> u32 {
    20
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<String>,
    #[serde(skip)]
    pub internal_note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings {
        Settings { theme: "light".into(), page_size: 20, bio: None, badges: vec![], internal_note: String::new() }
    }

    #[test]
    fn empty_object_gives_defaults() {
        let got: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(got, defaults(), "пустой объект должен давать значения по умолчанию, а не ошибку");
    }

    #[test]
    fn partial_object_keeps_what_was_sent() {
        let got: Settings = serde_json::from_str(r#"{"theme":"dark","badges":["new"]}"#).unwrap();
        assert_eq!(got.theme, "dark");
        assert_eq!(got.page_size, 20);
        assert_eq!(got.badges, vec!["new".to_string()]);
    }

    #[test]
    fn empty_values_stay_off_the_wire() {
        assert_eq!(serde_json::to_string(&defaults()).unwrap(), r#"{"theme":"light","page_size":20}"#);
        let full = Settings { bio: Some("hi".into()), badges: vec!["a".into()], ..defaults() };
        assert_eq!(
            serde_json::to_string(&full).unwrap(),
            r#"{"theme":"light","page_size":20,"bio":"hi","badges":["a"]}"#
        );
    }

    #[test]
    fn internal_note_never_crosses_the_wire() {
        let secret = Settings { internal_note: "banned for spam".into(), ..defaults() };
        assert!(!serde_json::to_string(&secret).unwrap().contains("spam"), "заметка утекла наружу");
        let got: Settings = serde_json::from_str(r#"{"internal_note":"i am admin"}"#).unwrap();
        assert_eq!(got.internal_note, "", "клиент не должен задавать служебное поле");
    }
}
