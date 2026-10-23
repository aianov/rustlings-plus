pub fn is_valid_username(name: &str) -> bool {
    let len = name.chars().count();
    if !(3..=20).contains(&len) {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    first.is_ascii_lowercase() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn is_plausible_email(email: &str) -> bool {
    if email.chars().any(char::is_whitespace) {
        return false;
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if local.is_empty() || local.chars().count() > 64 {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2 && labels.iter().all(|label| !label.is_empty())
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_usernames() {
        for name in ["alice", "bob_42", "a_b", "x1234567890123456789"] {
            assert!(is_valid_username(name), "{name} должен подходить");
        }
    }

    #[test]
    fn bad_usernames() {
        for name in ["ab", "Alice", "1alice", "_bob", "a b c", "алиса", "x12345678901234567890", "al-ice", ""] {
            assert!(!is_valid_username(name), "{name:?} не должен подходить");
        }
    }

    #[test]
    fn plausible_emails_are_accepted_without_extra_strictness() {
        for email in ["a@b.co", "alice@example.com", "a+tag@mail.example.co", "first.last@sub.example.org", "x_y-z@e-x.io"] {
            assert!(is_plausible_email(email), "{email} настоящий адрес, отказывать нельзя");
        }
    }

    #[test]
    fn broken_emails_are_rejected() {
        for email in ["", "a", "a@b", "@x.y", "a@@x.y", "a@x..y", "a@.x.y", "a@x.y.", "a b@x.y", "a@x y.z", "a@b@c.d"] {
            assert!(!is_plausible_email(email), "{email:?} не похож на адрес");
        }
    }

    #[test]
    fn local_part_length_is_limited() {
        assert!(is_plausible_email(&format!("{}@x.io", "a".repeat(64))), "64 символа локальной части допустимы");
        assert!(!is_plausible_email(&format!("{}@x.io", "a".repeat(65))), "65 уже много");
    }
}
