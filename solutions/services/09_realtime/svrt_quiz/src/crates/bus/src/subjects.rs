pub const REVOKED: &str = "session.revoked";

pub fn user_subject(user: &str) -> String {
    format!("user.{user}")
}

pub fn typing_subject(user: &str) -> String {
    format!("typing.{user}")
}

pub fn valid_word(word: &str) -> bool {
    !word.is_empty() && word.len() <= 32 && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
