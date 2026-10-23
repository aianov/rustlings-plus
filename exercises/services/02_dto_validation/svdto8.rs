// Формат ника и адреса почты проверяют руками. С ником всё понятно: строчная латиница, цифры и
// `_`, длина 3..=20, начинается с буквы. С почтой хуже: настоящий формат (RFC) чудовищен, и
// писать его целиком не нужно. Нужна проверка «похоже на адрес»: ровно один `@`, локальная часть
// не пустая и до 64 символов, домен из непустых меток через точку (меток не меньше двух: `a@b` не подходит), без пробелов. Строже не нужно:
// адрес `a+tag@mail.example.co` настоящий, и пользователю нельзя отказывать из-за плюса.

pub fn is_valid_username(name: &str) -> bool {
    todo!()
}

pub fn is_plausible_email(email: &str) -> bool {
    todo!()
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
