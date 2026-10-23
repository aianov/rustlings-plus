// В регистрации есть проверки: формат ника, занят ли он. Но в базе уже по два пользователя
// «alice» и «Alice», а иногда «alice » с пробелом, и пишутся жалобы про «двойников». Почта
// страдает так же. Тесты воспроизводят это. Цель: никакие два аккаунта не различаются только
// регистром или пробелами по краям, а в хранилище лежат значения в нормальном виде.

pub struct User {
    pub username: String,
    pub email: String,
}

#[derive(Debug, PartialEq)]
pub enum RegisterError {
    BadUsername,
    BadEmail,
    UsernameTaken,
    EmailTaken,
}

#[derive(Default)]
pub struct Registry {
    users: Vec<User>,
}

fn valid_username(name: &str) -> bool {
    let len = name.chars().count();
    (3..=20).contains(&len) && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn valid_email(email: &str) -> bool {
    matches!(email.split_once('@'), Some((l, d)) if !l.is_empty() && d.contains('.'))
}

impl Registry {
    pub fn register(&mut self, username: &str, email: &str) -> Result<(), RegisterError> {
        if !valid_username(username) {
            return Err(RegisterError::BadUsername);
        }
        if !valid_email(email) {
            return Err(RegisterError::BadEmail);
        }
        if self.users.iter().any(|u| u.username == username) {
            return Err(RegisterError::UsernameTaken);
        }
        if self.users.iter().any(|u| u.email == email) {
            return Err(RegisterError::EmailTaken);
        }
        self.users.push(User { username: username.to_string(), email: email.to_string() });
        Ok(())
    }

    pub fn users(&self) -> &[User] {
        &self.users
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_case_input_is_accepted_and_stored_normalized() {
        let mut registry = Registry::default();
        assert_eq!(registry.register("  Alice_1 ", " Alice@Example.COM "), Ok(()), "ввод с большими буквами и пробелами допустим");
        let user = &registry.users()[0];
        assert_eq!(
            (user.username.as_str(), user.email.as_str()),
            ("alice_1", "alice@example.com"),
            "в хранилище попадает нормальный вид"
        );
    }

    #[test]
    fn same_name_in_another_case_is_taken() {
        let mut registry = Registry::default();
        registry.register("alice", "a@x.io").unwrap();
        for variant in ["Alice", "ALICE", "alice ", " aLiCe"] {
            assert_eq!(
                registry.register(variant, &format!("{variant}@y.io")),
                Err(RegisterError::UsernameTaken),
                "{variant:?} это тот же ник, что alice"
            );
        }
        assert_eq!(registry.users().len(), 1, "двойников в хранилище нет");
    }

    #[test]
    fn same_email_in_another_case_is_taken() {
        let mut registry = Registry::default();
        registry.register("alice", "alice@example.com").unwrap();
        assert_eq!(
            registry.register("alice2", "ALICE@Example.com"),
            Err(RegisterError::EmailTaken),
            "почта без учёта регистра"
        );
    }

    #[test]
    fn validation_still_applies() {
        let mut registry = Registry::default();
        assert_eq!(registry.register("ab", "a@x.io"), Err(RegisterError::BadUsername), "ник слишком короткий");
        assert_eq!(registry.register("alice", "nope"), Err(RegisterError::BadEmail), "почта без @");
        assert_eq!(registry.register("ali ce", "a@x.io"), Err(RegisterError::BadUsername), "пробел внутри ника");
    }
}
