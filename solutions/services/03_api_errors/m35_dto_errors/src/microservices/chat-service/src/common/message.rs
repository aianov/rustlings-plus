use contracts::UserId;
use validation::{CharLen, NotBlank, Rule, RuleExt, Violation};

pub const MAX_MESSAGE_LEN: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub from: UserId,
    pub to: UserId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageText(String);

impl TryFrom<&str> for MessageText {
    type Error = Violation;

    fn try_from(raw: &str) -> Result<MessageText, Violation> {
        let text = raw.trim();
        let rule = NotBlank.and(CharLen {
            min: 1,
            max: MAX_MESSAGE_LEN,
        });
        rule.check(text)?;
        Ok(MessageText(text.to_string()))
    }
}

impl AsRef<str> for MessageText {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
