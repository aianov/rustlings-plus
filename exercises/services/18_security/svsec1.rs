// Форма входа отвечает «нет такого пользователя», когда имя не найдено, и «неверный пароль», когда имя есть, а пароль не тот. Для человека это
// удобство. Для атакующего это справочник: прогнал по списку имён, получил, кто у нас зарегистрирован, и дальше подбирает пароли только
// к ним. То же с «восстановлением пароля»: «письмо отправлено» и «такого адреса нет» выдают, кто среди пользователей.
// Цель: снаружи не должно быть видно, существует ли аккаунт. Вход отвечает одной и той же ошибкой и одним и тем же текстом в обоих случаях,
// восстановление пароля отвечает одним и тем же сообщением для любого адреса. Успешный вход работает как раньше.


use std::collections::{HashMap, HashSet};

fn main() {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    UnknownUser,
    WrongPassword,
    InvalidCredentials,
}

pub struct Users {
    passwords: HashMap<String, (u64, String)>,
    emails: HashSet<String>,
}

impl Users {
    pub fn new(accounts: &[(&str, u64, &str, &str)]) -> Users {
        Users {
            passwords: accounts.iter().map(|(name, id, pw, _)| (name.to_string(), (*id, format!("h:{pw}")))).collect(),
            emails: accounts.iter().map(|(_, _, _, email)| email.to_string()).collect(),
        }
    }
}

pub fn login(users: &Users, name: &str, password: &str) -> Result<u64, AuthError> {
    match users.passwords.get(name) {
        None => Err(AuthError::UnknownUser),
        Some((id, hash)) if *hash == format!("h:{password}") => Ok(*id),
        Some(_) => Err(AuthError::WrongPassword),
    }
}

/// Что видит клиент в теле ответа на ошибку входа.
pub fn error_body(e: &AuthError) -> String {
    match e {
        AuthError::UnknownUser => "Нет такого пользователя".to_string(),
        AuthError::WrongPassword => "Неверный пароль".to_string(),
        AuthError::InvalidCredentials => "Неверное имя или пароль".to_string(),
    }
}

/// Ответ на «забыл пароль» для адреса `email`.
pub fn reset_message(users: &Users, email: &str) -> String {
    if users.emails.contains(email) {
        "Письмо со ссылкой для сброса отправлено".to_string()
    } else {
        "Пользователь с таким адресом не найден".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users() -> Users {
        Users::new(&[("anna", 1, "secret", "anna@example.org"), ("boris", 2, "pa55", "boris@example.org")])
    }

    #[test]
    fn a_correct_login_works() {
        assert_eq!(login(&users(), "anna", "secret"), Ok(1));
        assert_eq!(login(&users(), "boris", "pa55"), Ok(2));
    }

    #[test]
    fn unknown_user_and_wrong_password_look_the_same() {
        let u = users();
        let ghost = login(&u, "ghost", "secret");
        let wrong = login(&u, "anna", "не тот");
        assert_eq!(ghost, Err(AuthError::InvalidCredentials));
        assert_eq!(ghost, wrong, "ошибка для несуществующего имени отличается от ошибки для неверного пароля: по ней видно, кто зарегистрирован");
    }

    #[test]
    fn the_response_text_is_the_same_too() {
        let u = users();
        let a = error_body(&login(&u, "ghost", "x").unwrap_err());
        let b = error_body(&login(&u, "anna", "x").unwrap_err());
        assert_eq!(a, b, "тексты ответов различаются: {a:?} и {b:?}");
        assert!(!a.to_lowercase().contains("нет такого") && !a.to_lowercase().contains("не найден"), "в тексте не должно быть намёка на существование: {a:?}");
    }

    #[test]
    fn the_reset_message_does_not_reveal_accounts() {
        let u = users();
        let known = reset_message(&u, "anna@example.org");
        let unknown = reset_message(&u, "nobody@example.org");
        assert_eq!(known, unknown, "восстановление пароля отвечает по-разному для известного и неизвестного адреса");
        assert!(!known.is_empty());
    }
}
