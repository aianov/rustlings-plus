use std::fmt;

use serde::{Deserialize, Serialize};
use validation::Violation;

pub const MAX_EMAIL_LEN: usize = 254;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailError {
    Blank,
    TooLong,
    Format,
}

impl fmt::Display for EmailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmailError::Blank => write!(f, "почта не может быть пустой"),
            EmailError::TooLong => write!(f, "почта длиннее {MAX_EMAIL_LEN} символов"),
            EmailError::Format => write!(f, "это не похоже на адрес почты"),
        }
    }
}

impl std::error::Error for EmailError {}

impl From<EmailError> for Violation {
    fn from(error: EmailError) -> Violation {
        let code = match error {
            EmailError::Blank => "blank",
            EmailError::TooLong => "too_long",
            EmailError::Format => "invalid_format",
        };
        Violation::new(code, error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Email(String);

fn label_ok(label: &str) -> bool {
    !label.is_empty()
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl TryFrom<&str> for Email {
    type Error = EmailError;

    fn try_from(raw: &str) -> Result<Email, EmailError> {
        let email = raw.trim().to_ascii_lowercase();
        if email.is_empty() {
            return Err(EmailError::Blank);
        }
        if email.chars().count() > MAX_EMAIL_LEN {
            return Err(EmailError::TooLong);
        }
        let Some((local, domain)) = email.split_once('@') else {
            return Err(EmailError::Format);
        };
        let local_ok = !local.is_empty()
            && local.len() <= 64
            && local
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c));
        let labels: Vec<&str> = domain.split('.').collect();
        let domain_ok = labels.len() >= 2 && labels.iter().all(|label| label_ok(label));
        if local_ok && domain_ok {
            Ok(Email(email))
        } else {
            Err(EmailError::Format)
        }
    }
}

impl TryFrom<String> for Email {
    type Error = EmailError;

    fn try_from(raw: String) -> Result<Email, EmailError> {
        Email::try_from(raw.as_str())
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<Email> for String {
    fn from(email: Email) -> String {
        email.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
