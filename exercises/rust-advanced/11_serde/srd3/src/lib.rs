// Настройки приходят от клиентов частично: кто-то присылает только тему, кто-то пустой объект `{}`. Нужны
// значения по умолчанию, причём не «пустые», а осмысленные (тема `light`, страница по 20 постов). В обратную
// сторону лишнего писать не стоит: пустые био и список значков не нужны на проводе, а служебная заметка
// `internal_note` не должна ни покидать сервер, ни приниматься от клиента, даже если он её пришлёт.
// TODO: Расставь атрибуты на полях `Settings`. Функции для значений по умолчанию уже есть.
use serde::{Deserialize, Serialize};

fn default_theme() -> String {
    "light".to_string()
}

fn default_page_size() -> u32 {
    20
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub theme: String,
    pub page_size: u32,
    pub bio: Option<String>,
    pub badges: Vec<String>,
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
