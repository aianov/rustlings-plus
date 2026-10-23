use std::fmt;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub struct InvalidValue {
    pub name: String,
    pub value: String,
    pub expected: &'static str,
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "переменная {}: значение {:?} не подходит, ожидается {}", self.name, self.value, self.expected)
    }
}

pub fn parse_or<T: FromStr>(
    name: &str,
    raw: Option<&str>,
    default: T,
    expected: &'static str,
) -> Result<T, InvalidValue> {
    match raw {
        None => Ok(default),
        Some(value) => value.trim().parse().map_err(|_| InvalidValue {
            name: name.to_string(),
            value: value.to_string(),
            expected,
        }),
    }
}

pub fn parse_port(name: &str, raw: Option<&str>, default: u16) -> Result<u16, InvalidValue> {
    let port = parse_or(name, raw, default, "порт 1..=65535")?;
    if port == 0 {
        return Err(InvalidValue {
            name: name.to_string(),
            value: raw.unwrap_or_default().to_string(),
            expected: "порт 1..=65535",
        });
    }
    Ok(port)
}

pub fn parse_millis(name: &str, raw: Option<&str>, default: Duration) -> Result<Duration, InvalidValue> {
    let millis = parse_or(name, raw, default.as_millis() as u64, "целое число миллисекунд")?;
    Ok(Duration::from_millis(millis))
}

pub fn parse_bool(name: &str, raw: Option<&str>, default: bool) -> Result<bool, InvalidValue> {
    let Some(value) = raw else {
        return Ok(default);
    };
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(InvalidValue {
            name: name.to_string(),
            value: value.to_string(),
            expected: "true или false",
        }),
    }
}

fn main() {
    println!("{:?}", parse_port("PORT", Some("8080"), 80));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_variable_gets_the_default() {
        assert_eq!(parse_port("PORT", None, 8080), Ok(8080), "нет переменной: берём умолчание");
        assert_eq!(parse_bool("DEBUG", None, true), Ok(true), "для флага тоже");
        assert_eq!(
            parse_millis("TIMEOUT_MS", None, Duration::from_secs(5)),
            Ok(Duration::from_secs(5)),
            "и для таймаута"
        );
    }

    #[test]
    fn present_variable_wins_over_the_default() {
        assert_eq!(parse_port("PORT", Some(" 9000 "), 8080), Ok(9000), "пробелы вокруг числа не мешают");
        assert_eq!(
            parse_millis("TIMEOUT_MS", Some("250"), Duration::from_secs(5)),
            Ok(Duration::from_millis(250)),
            "250 миллисекунд"
        );
        assert_eq!(parse_or("RETRIES", Some("7"), 3u32, "число"), Ok(7), "дженерик работает для любого FromStr");
    }

    #[test]
    fn garbage_is_an_error_not_a_silent_default() {
        let err = parse_port("PORT", Some("abc"), 8080).unwrap_err();
        assert_eq!(err.name, "PORT", "ошибка называет переменную");
        assert_eq!(err.value, "abc", "и значение, которое не подошло");
        let text = err.to_string();
        assert!(text.contains("PORT") && text.contains("abc"), "в тексте должны быть имя и значение: {text}");
        assert!(parse_port("PORT", Some("70000"), 8080).is_err(), "70000 не помещается в u16");
        assert!(parse_port("PORT", Some("0"), 8080).is_err(), "порт 0 не годится для сервиса");
        assert!(parse_port("PORT", Some("-1"), 8080).is_err(), "отрицательный порт");
        assert!(parse_millis("T", Some("1.5"), Duration::ZERO).is_err(), "дробные миллисекунды не принимаем");
    }

    #[test]
    fn booleans_accept_the_usual_spellings() {
        for yes in ["true", "TRUE", "1", "yes", "on"] {
            assert_eq!(parse_bool("F", Some(yes), false), Ok(true), "{yes} это истина");
        }
        for no in ["false", "False", "0", "no", "off"] {
            assert_eq!(parse_bool("F", Some(no), true), Ok(false), "{no} это ложь");
        }
        let err = parse_bool("DEBUG", Some("maybe"), false).unwrap_err();
        assert_eq!(err.value, "maybe", "ошибка показывает исходную строку");
    }
}
