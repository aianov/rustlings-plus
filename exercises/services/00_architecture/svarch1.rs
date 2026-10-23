// Регистрация написана комком: разбор строки, правило ника, хранение и текст ответа живут в
// одной функции, и проверить правило без списка пользователей и без команды нельзя.
// Цель: правило ника существует отдельной чистой функцией `validate_username` (ник
// обрезается, приводится к нижнему регистру, 3..=20 символов, допустимы a-z, 0-9 и `_`), а
// `handle_line` пользуется ею и больше ничего о правиле не знает. Поведение `handle_line` прежнее.

#[derive(Debug, PartialEq)]
pub enum UsernameError {
    TooShort,
    TooLong,
    BadChar(char),
}

pub fn validate_username(raw: &str) -> Result<String, UsernameError> {
    todo!()
}

pub fn handle_line(store: &mut Vec<String>, line: &str) -> String {
    let Some(raw) = line.strip_prefix("register ") else {
        return "400 expected: register <name>".to_string();
    };
    let name = raw.trim().to_lowercase();
    let len = name.chars().count();
    if len < 3 {
        return "422 username too short".to_string();
    }
    if len > 20 {
        return "422 username too long".to_string();
    }
    if let Some(c) = name.chars().find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')) {
        return format!("422 bad character '{c}'");
    }
    if store.contains(&name) {
        return "409 username taken".to_string();
    }
    store.push(name.clone());
    format!("201 {name}")
}

fn main() {
    let mut store = Vec::new();
    println!("{}", handle_line(&mut store, "register Alice"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_name_is_normalized() {
        assert_eq!(
            validate_username("  Alice_01 "),
            Ok("alice_01".to_string()),
            "ник нужно обрезать по краям и привести к нижнему регистру"
        );
    }

    #[test]
    fn length_is_counted_in_chars() {
        assert_eq!(validate_username("ab"), Err(UsernameError::TooShort), "два символа это мало");
        assert_eq!(validate_username("abc"), Ok("abc".to_string()), "три символа это минимум");
        assert_eq!(
            validate_username(&"a".repeat(20)),
            Ok("a".repeat(20)),
            "двадцать символов это максимум"
        );
        assert_eq!(
            validate_username(&"a".repeat(21)),
            Err(UsernameError::TooLong),
            "двадцать один символ уже слишком много"
        );
    }

    #[test]
    fn bad_characters_are_reported() {
        assert_eq!(
            validate_username("ali ce"),
            Err(UsernameError::BadChar(' ')),
            "пробел внутри ника недопустим, ошибка должна назвать символ"
        );
        assert_eq!(
            validate_username("алиса"),
            Err(UsernameError::BadChar('а')),
            "кириллица не входит в a-z, ошибка называет первый плохой символ"
        );
    }

    #[test]
    fn handler_keeps_its_behaviour() {
        let mut store = Vec::new();
        assert_eq!(handle_line(&mut store, "register Alice"), "201 alice", "успешная регистрация");
        assert_eq!(handle_line(&mut store, "register alice"), "409 username taken", "ник занят");
        assert_eq!(handle_line(&mut store, "register al"), "422 username too short", "слишком короткий");
        assert_eq!(handle_line(&mut store, "register a-b-c"), "422 bad character '-'", "плохой символ");
        assert_eq!(handle_line(&mut store, "hello"), "400 expected: register <name>", "неизвестная команда");
        assert_eq!(store, vec!["alice".to_string()], "в хранилище только успешные регистрации");
    }

    #[test]
    fn handler_does_not_duplicate_the_rule() {
        let source = include_str!("svarch1.rs");
        let start = source.find("pub fn handle_line").unwrap();
        let end = source.find("fn main()").unwrap();
        let body = &source[start..end];
        assert!(
            !body.contains("chars().count()") && !body.contains("to_lowercase"),
            "правило ника всё ещё живёт внутри handle_line: пусть он зовёт validate_username"
        );
    }
}
