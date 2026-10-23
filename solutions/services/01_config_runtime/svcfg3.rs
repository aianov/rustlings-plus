use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Missing(String),
    Invalid { name: String, value: String, expected: &'static str },
}

#[derive(Debug, PartialEq)]
pub struct Config {
    pub port: u16,
    pub request_timeout: Duration,
    pub debug: bool,
    pub database_url: String,
}

fn invalid(name: &str, value: &str, expected: &'static str) -> ConfigError {
    ConfigError::Invalid { name: name.to_string(), value: value.to_string(), expected }
}

fn port(name: &str, raw: Option<String>, default: u16) -> Result<u16, ConfigError> {
    match raw {
        None => Ok(default),
        Some(v) => match v.trim().parse::<u16>() {
            Ok(p) if p != 0 => Ok(p),
            _ => Err(invalid(name, &v, "порт 1..=65535")),
        },
    }
}

fn millis(name: &str, raw: Option<String>, default: u64) -> Result<Duration, ConfigError> {
    match raw {
        None => Ok(Duration::from_millis(default)),
        Some(v) => v
            .trim()
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| invalid(name, &v, "миллисекунды")),
    }
}

fn flag(name: &str, raw: Option<String>, default: bool) -> Result<bool, ConfigError> {
    match raw.as_deref().map(str::trim) {
        None => Ok(default),
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        Some(other) => Err(invalid(name, other, "true или false")),
    }
}

impl Config {
    pub fn from_source(get: impl Fn(&str) -> Option<String>) -> Result<Config, ConfigError> {
        Ok(Config {
            port: port("PORT", get("PORT"), 8080)?,
            request_timeout: millis("REQUEST_TIMEOUT_MS", get("REQUEST_TIMEOUT_MS"), 5000)?,
            debug: flag("DEBUG", get("DEBUG"), false)?,
            database_url: get("DATABASE_URL").ok_or_else(|| ConfigError::Missing("DATABASE_URL".to_string()))?,
        })
    }

    pub fn from_env() -> Result<Config, ConfigError> {
        Config::from_source(|name| std::env::var(name).ok())
    }
}

fn main() {
    println!("{:?}", Config::from_env());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn source(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn defaults_fill_the_gaps() {
        let cfg = Config::from_source(source(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(cfg.port, 8080, "порт по умолчанию 8080");
        assert_eq!(cfg.request_timeout, Duration::from_secs(5), "таймаут по умолчанию 5 секунд");
        assert!(!cfg.debug, "debug по умолчанию выключен");
        assert_eq!(cfg.database_url, "postgres://x", "адрес базы берётся из источника");
    }

    #[test]
    fn values_override_defaults() {
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "postgres://y"),
            ("PORT", "9001"),
            ("REQUEST_TIMEOUT_MS", "250"),
            ("DEBUG", "true"),
        ]))
        .unwrap();
        assert_eq!((cfg.port, cfg.debug), (9001, true), "PORT и DEBUG из источника");
        assert_eq!(cfg.request_timeout, Duration::from_millis(250), "таймаут из источника");
    }

    #[test]
    fn required_variable_is_required() {
        let err = Config::from_source(source(&[("PORT", "1")])).unwrap_err();
        assert_eq!(err, ConfigError::Missing("DATABASE_URL".to_string()), "без DATABASE_URL конфиг не собирается");
    }

    #[test]
    fn bad_value_names_the_variable() {
        let err = Config::from_source(source(&[("DATABASE_URL", "x"), ("PORT", "abc")])).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Invalid { name, value, .. } if name == "PORT" && value == "abc"),
            "ошибка должна называть PORT и значение abc, а пришло {err:?}"
        );
    }

    #[test]
    fn two_configs_from_two_sources_do_not_interfere() {
        let a = Config::from_source(source(&[("DATABASE_URL", "a"), ("PORT", "1111")])).unwrap();
        let b = Config::from_source(source(&[("DATABASE_URL", "b"), ("PORT", "2222")])).unwrap();
        assert_eq!((a.port, b.port), (1111, 2222), "каждый конфиг видит только свой источник");
    }
}
