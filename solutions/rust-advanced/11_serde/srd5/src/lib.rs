use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Comment { post: u64, text: String },
    Share { post: u64, to: u64 },
    Like { post: u64 },
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
