// Клиент присылает три вида входящих действий без всякого тега: просто объект с нужными полями. Лайк это
// `{"post": 1}`, репост `{"post": 1, "to": 7}`, комментарий `{"post": 1, "text": "hi"}`. Сервер читает их
// через `untagged`, но в логах все комментарии и репосты оказываются лайками, и текст теряется. Сериализация
// в порядке, портится только чтение.
// TODO: Найди, почему serde выбирает не тот вариант, и сделай так, чтобы каждое действие читалось как своё.
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Like { post: u64 },
    Share { post: u64, to: u64 },
    Comment { post: u64, text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(raw: &str) -> Incoming {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn like_stays_a_like() {
        assert_eq!(read(r#"{"post":1}"#), Incoming::Like { post: 1 });
    }

    #[test]
    fn comment_keeps_its_text() {
        assert_eq!(read(r#"{"post":1,"text":"hi"}"#), Incoming::Comment { post: 1, text: "hi".into() });
        assert_eq!(read(r#"{"text":"later","post":2}"#), Incoming::Comment { post: 2, text: "later".into() });
    }

    #[test]
    fn share_keeps_its_target() {
        assert_eq!(read(r#"{"post":1,"to":7}"#), Incoming::Share { post: 1, to: 7 }, "репост не должен стать лайком");
    }

    #[test]
    fn roundtrip_for_every_variant() {
        for item in [
            Incoming::Like { post: 4 },
            Incoming::Share { post: 5, to: 6 },
            Incoming::Comment { post: 7, text: "x".into() },
        ] {
            let json = serde_json::to_string(&item).unwrap();
            assert_eq!(serde_json::from_str::<Incoming>(&json).unwrap(), item, "{json} прочитался как другое действие");
        }
    }

    #[test]
    fn nothing_matches_garbage() {
        assert!(serde_json::from_str::<Incoming>(r#"{"nothing":1}"#).is_err());
    }
}
