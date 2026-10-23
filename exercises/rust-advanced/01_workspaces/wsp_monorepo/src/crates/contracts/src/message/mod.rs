use crate::ids::UserId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: UserId,
    pub text: String,
}

impl Message {
    pub fn new(from: UserId, text: &str) -> Message {
        Message { from, text: text.to_string() }
    }

    pub fn to_json(&self) -> String {
        serde_json::json!({ "from": self.from.0, "text": self.text }).to_string()
    }

    pub fn from_json(text: &str) -> Option<Message> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(Message::new(UserId(value["from"].as_u64()?), value["text"].as_str()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let m = Message::new(UserId(7), "hi \"there\"");
        assert_eq!(Message::from_json(&m.to_json()), Some(m));
    }
}
