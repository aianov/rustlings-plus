use std::env::VarError;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Missing(String),
    NotUnicode(String),
    Empty(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(f, "переменная {name} не задана"),
            ConfigError::NotUnicode(name) => write!(f, "переменная {name} содержит не UTF-8"),
            ConfigError::Empty(name) => write!(f, "переменная {name} пустая"),
        }
    }
}

pub fn read_required(
    name: &str,
    lookup: impl Fn(&str) -> Result<String, VarError>,
) -> Result<String, ConfigError> {
    match lookup(name) {
        Ok(value) if value.trim().is_empty() => Err(ConfigError::Empty(name.to_string())),
        Ok(value) => Ok(value),
        Err(VarError::NotPresent) => Err(ConfigError::Missing(name.to_string())),
        Err(VarError::NotUnicode(_)) => Err(ConfigError::NotUnicode(name.to_string())),
    }
}

fn main() {
    println!("{:?}", read_required("HOME", |n| std::env::var(n)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn present_value_is_returned_as_is() {
        let got = read_required("PORT", |_| Ok(" 8080 ".to_string()));
        assert_eq!(got, Ok(" 8080 ".to_string()), "значение отдаётся как есть, обрезать его не наше дело");
    }

    #[test]
    fn missing_is_not_the_same_as_garbage() {
        let missing = read_required("DB", |_| Err(VarError::NotPresent));
        assert_eq!(missing, Err(ConfigError::Missing("DB".to_string())), "нет переменной: Missing");
        let bad = OsString::from_vec(vec![0xff, 0xfe]);
        let garbage = read_required("DB", |_| Err(VarError::NotUnicode(bad.clone())));
        assert_eq!(
            garbage,
            Err(ConfigError::NotUnicode("DB".to_string())),
            "в переменной не UTF-8: это NotUnicode, а не Missing"
        );
    }

    #[test]
    fn blank_value_is_its_own_error() {
        for blank in ["", "   ", "\t"] {
            let got = read_required("TOKEN", |_| Ok(blank.to_string()));
            assert_eq!(got, Err(ConfigError::Empty("TOKEN".to_string())), "{blank:?} это пустое значение");
        }
    }

    #[test]
    fn messages_name_the_variable() {
        let text = ConfigError::Missing("DATABASE_URL".to_string()).to_string();
        assert!(text.contains("DATABASE_URL"), "сообщение должно называть переменную, а было: {text}");
    }

    #[test]
    fn real_environment_works_too() {
        let got = read_required("SVCFG1_SURELY_NOT_SET_ANYWHERE", |n| std::env::var(n));
        assert_eq!(
            got,
            Err(ConfigError::Missing("SVCFG1_SURELY_NOT_SET_ANYWHERE".to_string())),
            "настоящее окружение без такой переменной даёт Missing"
        );
    }
}
