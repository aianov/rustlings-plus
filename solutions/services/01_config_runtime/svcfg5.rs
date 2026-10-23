use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Missing(String),
    Invalid { name: String, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(f, "{name}: не задана"),
            ConfigError::Invalid { name, value } => write!(f, "{name}: неверное значение {value:?}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Config {
    pub port: u16,
    pub workers: usize,
    pub database_url: String,
    pub cache_url: String,
}

fn number<T: std::str::FromStr>(name: &str, raw: Option<String>, default: T) -> Result<T, ConfigError> {
    match raw {
        None => Ok(default),
        Some(v) => v.trim().parse().map_err(|_| ConfigError::Invalid { name: name.to_string(), value: v }),
    }
}

fn required(name: &str, raw: Option<String>) -> Result<String, ConfigError> {
    raw.ok_or_else(|| ConfigError::Missing(name.to_string()))
}

impl Config {
    pub fn from_source(get: impl Fn(&str) -> Option<String>) -> Result<Config, Vec<ConfigError>> {
        let port = number("PORT", get("PORT"), 8080u16);
        let workers = number("WORKERS", get("WORKERS"), 4usize);
        let database_url = required("DATABASE_URL", get("DATABASE_URL"));
        let cache_url = required("CACHE_URL", get("CACHE_URL"));
        match (port, workers, database_url, cache_url) {
            (Ok(port), Ok(workers), Ok(database_url), Ok(cache_url)) => {
                Ok(Config { port, workers, database_url, cache_url })
            }
            (port, workers, database_url, cache_url) => Err([
                port.err(),
                workers.err(),
                database_url.err(),
                cache_url.err(),
            ]
            .into_iter()
            .flatten()
            .collect()),
        }
    }
}

pub fn render_errors(errors: &[ConfigError]) -> String {
    errors.iter().map(|e| format!("* {e}\n")).collect()
}

fn main() {
    if let Err(errors) = Config::from_source(|n| std::env::var(n).ok()) {
        eprint!("{}", render_errors(&errors));
    }
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
    fn good_config_builds() {
        let cfg = Config::from_source(source(&[("DATABASE_URL", "db"), ("CACHE_URL", "cache")])).unwrap();
        assert_eq!((cfg.port, cfg.workers), (8080, 4), "умолчания");
        assert_eq!((cfg.database_url.as_str(), cfg.cache_url.as_str()), ("db", "cache"), "обязательные значения");
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let errors = Config::from_source(source(&[("PORT", "http"), ("WORKERS", "-3")])).unwrap_err();
        assert_eq!(
            errors,
            vec![
                ConfigError::Invalid { name: "PORT".to_string(), value: "http".to_string() },
                ConfigError::Invalid { name: "WORKERS".to_string(), value: "-3".to_string() },
                ConfigError::Missing("DATABASE_URL".to_string()),
                ConfigError::Missing("CACHE_URL".to_string()),
            ],
            "должны вернуться все четыре ошибки, в порядке полей конфига"
        );
    }

    #[test]
    fn one_problem_is_one_error() {
        let errors = Config::from_source(source(&[("DATABASE_URL", "db")])).unwrap_err();
        assert_eq!(errors, vec![ConfigError::Missing("CACHE_URL".to_string())], "остальное в порядке");
    }

    #[test]
    fn report_is_one_line_per_error() {
        let errors = vec![ConfigError::Missing("A".to_string()), ConfigError::Missing("B".to_string())];
        assert_eq!(
            render_errors(&errors),
            "* A: не задана\n* B: не задана\n",
            "по строке на ошибку, каждая с «* » и переводом строки"
        );
        assert_eq!(render_errors(&[]), "", "ошибок нет: текст пустой");
    }
}
