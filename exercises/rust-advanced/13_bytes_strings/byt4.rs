// Регистрация проверяет два поля, и правила у них разные. Логин потом попадает в ссылки, имена файлов
// и командную строку, поэтому только ASCII: латинские буквы, цифры и подчёркивание, от 3 до 20 символов,
// не начинается с цифры. Отображаемое имя это человеческий текст на любом языке: буквы любого алфавита,
// цифры, пробел, дефис и апостроф, от 1 до 40 символов, без пробелов по краям. У `char` есть методы
// `is_alphanumeric` (любой алфавит) и `is_ascii_alphanumeric` (только ASCII), и перепутать их легко.
// TODO: Допиши обе проверки.
pub fn valid_login(login: &str) -> bool {
    todo!("правила логина")
}

pub fn valid_display_name(name: &str) -> bool {
    todo!("правила отображаемого имени")
}

fn main() {
    println!("{}", valid_login("ira_k"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_logins_pass() {
        for ok in ["ira", "ira_k", "User_42", "a_b", "x".repeat(20).as_str()] {
            assert!(valid_login(ok), "{ok} должен подойти");
        }
    }

    #[test]
    fn logins_are_ascii_only() {
        assert!(!valid_login("Ира"), "кириллица в логине не допускается");
        assert!(!valid_login("user１２"), "полноширинные цифры выглядят как цифры, но это не ASCII");
        assert!(!valid_login("naïve_user"));
        assert!(!valid_login("bob smith"));
        assert!(!valid_login("bob-smith"));
    }

    #[test]
    fn login_length_and_first_char() {
        assert!(!valid_login("ab"));
        assert!(!valid_login(&"x".repeat(21)));
        assert!(!valid_login("1abc"), "с цифры начинать нельзя");
        assert!(valid_login("_abc"));
        assert!(!valid_login(""));
    }

    #[test]
    fn display_names_may_be_in_any_language() {
        for ok in ["Анна-Мария", "Zoë O'Neil", "李雷", "Bob 2", "Σωκράτης"] {
            assert!(valid_display_name(ok), "{ok} должно подойти");
        }
    }

    #[test]
    fn display_names_reject_junk() {
        assert!(!valid_display_name(""));
        assert!(!valid_display_name(" Bob"));
        assert!(!valid_display_name("Bob "));
        assert!(!valid_display_name("Bob!"));
        assert!(!valid_display_name("a\nb"));
        assert!(!valid_display_name("😀"));
    }

    #[test]
    fn display_name_length_counts_chars_not_bytes() {
        assert!(valid_display_name(&"ж".repeat(40)), "40 букв это 80 байт, но 40 символов");
        assert!(!valid_display_name(&"ж".repeat(41)));
    }
}
