
fn main() {}

pub fn log_line(level: &str, user: &str, msg: &str) -> String {
    format!("level={level} user=\"{}\" msg=\"{}\"", escape(user), escape(msg))
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Разбор строки журнала обратно в (уровень, пользователь, сообщение). Не принимает ничего лишнего.
pub fn parse_line(line: &str) -> Option<(String, String, String)> {
    let rest = line.strip_prefix("level=")?;
    let (level, rest) = rest.split_once(" user=\"")?;
    let (user, rest) = read_quoted(rest)?;
    let rest = rest.strip_prefix(" msg=\"")?;
    let (msg, rest) = read_quoted(rest)?;
    rest.is_empty().then(|| (level.to_string(), user, msg))
}

fn read_quoted(s: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &s[i + 1..])),
            '\\' => match chars.next()?.1 {
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    if chars.next()?.1 != '{' {
                        return None;
                    }
                    let mut hex = String::new();
                    loop {
                        match chars.next()?.1 {
                            '}' => break,
                            h => hex.push(h),
                        }
                    }
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c if c.is_control() => return None,
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_line_roundtrips() {
        let line = log_line("INFO", "anna", "вошла в систему");
        assert_eq!(parse_line(&line), Some(("INFO".into(), "anna".into(), "вошла в систему".into())));
    }

    #[test]
    fn a_line_break_does_not_create_a_second_record() {
        let evil = "анна\" msg=\"вход выполнен\"\nlevel=INFO user=\"admin\" msg=\"пароль изменён";
        let line = log_line("INFO", evil, "x");
        assert!(!line.contains('\n') && !line.contains('\r'), "в журнал ушла не одна строка: {line:?}");
        assert_eq!(parse_line(&line), Some(("INFO".into(), evil.into(), "x".into())), "разбор должен вернуть ровно то, что записали");
    }

    #[test]
    fn quotes_backslashes_and_control_characters_survive() {
        for nasty in ["a\"b", "a\\b", "a\\\"b", "tab\tinside", "\u{1b}[31mred", "nul\u{0}byte", "\r\n", "юникод ёж 🚀", "\\n", "конец\\"] {
            let line = log_line("WARN", nasty, nasty);
            assert!(!line.chars().any(|c| c.is_control()), "в строке остались управляющие символы: {line:?}");
            assert_eq!(parse_line(&line), Some(("WARN".into(), nasty.into(), nasty.into())), "значение {nasty:?} не восстановилось после записи и разбора");
        }
    }

    #[test]
    fn escaping_is_injective() {
        let a = log_line("INFO", "a\nb", "m");
        let b = log_line("INFO", "a\\nb", "m");
        assert_ne!(a, b, "перевод строки и текст «\\n» дали одинаковую запись: по журналу их не различить");
    }
}
