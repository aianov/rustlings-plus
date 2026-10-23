use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Reaction {
    Like,
    Emoji(String),
    Custom { code: u32, name: String },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Follow { user: u64 },
    Unfollow { user: u64 },
    Ping,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "d", rename_all = "lowercase")]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16 },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Id(u64),
    Name(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn externally_tagged_by_default() {
        assert_eq!(json(&Reaction::Like), r#""Like""#, "вариант без данных это просто строка");
        assert_eq!(json(&Reaction::Emoji("ok".into())), r#"{"Emoji":"ok"}"#);
        assert_eq!(json(&Reaction::Custom { code: 1, name: "x".into() }), r#"{"Custom":{"code":1,"name":"x"}}"#);
    }

    #[test]
    fn command_has_its_tag_inside() {
        assert_eq!(json(&Command::Follow { user: 5 }), r#"{"cmd":"follow","user":5}"#);
        assert_eq!(json(&Command::Ping), r#"{"cmd":"ping"}"#);
        let got: Command = serde_json::from_str(r#"{"user":9,"cmd":"unfollow"}"#).unwrap();
        assert_eq!(got, Command::Unfollow { user: 9 }, "тег может стоять где угодно в объекте");
    }

    #[test]
    fn frame_keeps_tag_and_data_apart() {
        assert_eq!(json(&Frame::Text("hi".into())), r#"{"t":"text","d":"hi"}"#);
        assert_eq!(json(&Frame::Binary(vec![1, 2])), r#"{"t":"binary","d":[1,2]}"#);
        assert_eq!(json(&Frame::Close { code: 1000 }), r#"{"t":"close","d":{"code":1000}}"#);
        let got: Frame = serde_json::from_str(r#"{"t":"close","d":{"code":4000}}"#).unwrap();
        assert_eq!(got, Frame::Close { code: 4000 });
    }

    #[test]
    fn target_is_just_a_number_or_a_string() {
        assert_eq!(json(&Target::Id(5)), "5");
        assert_eq!(json(&Target::Name("ira".into())), r#""ira""#);
        assert_eq!(serde_json::from_str::<Target>("42").unwrap(), Target::Id(42));
        assert_eq!(serde_json::from_str::<Target>(r#""bob""#).unwrap(), Target::Name("bob".into()));
        assert!(serde_json::from_str::<Target>("true").is_err(), "ни число, ни строка: не подходит");
    }
}
