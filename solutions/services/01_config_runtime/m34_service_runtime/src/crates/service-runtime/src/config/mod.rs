mod dotenv;
mod secret;

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

pub use dotenv::parse_dotenv;
pub use secret::Secret;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Missing {
        key: String,
    },
    Invalid {
        key: String,
        value: String,
        reason: String,
    },
    Dotenv {
        line: usize,
    },
    Unreadable {
        path: String,
        reason: String,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Missing { key } => write!(f, "{key}: не задана"),
            Problem::Invalid { key, value, reason } => {
                write!(f, "{key}: значение {value:?} не подходит ({reason})")
            }
            Problem::Dotenv { line } => write!(f, ".env, строка {line}: ждали КЛЮЧ=значение"),
            Problem::Unreadable { path, reason } => write!(f, "{path}: не читается ({reason})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub problems: Vec<Problem>,
}

impl ConfigError {
    pub fn keys(&self) -> Vec<&str> {
        self.problems
            .iter()
            .filter_map(|problem| match problem {
                Problem::Missing { key } | Problem::Invalid { key, .. } => Some(key.as_str()),
                Problem::Dotenv { .. } | Problem::Unreadable { .. } => None,
            })
            .collect()
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ошибок в настройках: {}", self.problems.len())?;
        for problem in &self.problems {
            write!(f, "\n  {problem}")?;
        }
        Ok(())
    }
}

impl Error for ConfigError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    vars: HashMap<String, String>,
}

impl Source {
    pub fn new() -> Source {
        Source::default()
    }

    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Source
    where
        K: Into<String>,
        V: Into<String>,
    {
        Source {
            vars: pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    pub fn with(mut self, key: &str, value: &str) -> Source {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }

    pub fn process() -> Source {
        Source::from_pairs(
            env::vars_os().filter_map(|(key, value)| {
                Some((key.into_string().ok()?, value.into_string().ok()?))
            }),
        )
    }

    pub fn from_dotenv(path: &Path) -> Result<Source, ConfigError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Source::new()),
            Err(error) => {
                return Err(ConfigError {
                    problems: vec![Problem::Unreadable {
                        path: path.display().to_string(),
                        reason: error.to_string(),
                    }],
                });
            }
        };
        parse_dotenv(&text)
            .map(Source::from_pairs)
            .map_err(|problems| ConfigError { problems })
    }

    pub fn load(dotenv: impl AsRef<Path>) -> Result<Source, ConfigError> {
        Ok(Source::from_dotenv(dotenv.as_ref())?.over(Source::process()))
    }

    pub fn over(mut self, top: Source) -> Source {
        self.vars.extend(top.vars);
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    }

    pub fn fields(&self) -> Fields<'_> {
        Fields {
            source: self,
            problems: Vec::new(),
        }
    }
}

pub struct Fields<'a> {
    source: &'a Source,
    problems: Vec<Problem>,
}

impl Fields<'_> {
    fn invalid(&mut self, key: &str, value: &str, reason: impl fmt::Display) {
        self.problems.push(Problem::Invalid {
            key: key.to_string(),
            value: value.to_string(),
            reason: reason.to_string(),
        });
    }

    fn convert<T>(&mut self, key: &str, read: impl FnOnce(&str) -> Result<T, String>) -> Option<T> {
        let raw = self.source.get(key)?;
        match read(raw) {
            Ok(value) => Some(value),
            Err(reason) => {
                self.invalid(key, raw, reason);
                None
            }
        }
    }

    pub fn optional<T>(&mut self, key: &str) -> Option<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        self.convert(key, |raw| {
            raw.parse().map_err(|error: T::Err| error.to_string())
        })
    }

    pub fn parse<T>(&mut self, key: &str, default: T) -> T
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        self.optional(key).unwrap_or(default)
    }

    pub fn required<T>(&mut self, key: &str) -> Option<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        if self.source.get(key).is_none() {
            self.problems.push(Problem::Missing {
                key: key.to_string(),
            });
            return None;
        }
        self.optional(key)
    }

    pub fn text(&mut self, key: &str, default: &str) -> String {
        self.source.get(key).unwrap_or(default).to_string()
    }

    pub fn addr(&mut self, key: &str, default: &str) -> String {
        self.convert(key, |raw| match raw.rsplit_once(':') {
            Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => {
                Ok(raw.to_string())
            }
            _ => Err("ждали хост:порт, например 127.0.0.1:50052".to_string()),
        })
        .unwrap_or_else(|| default.to_string())
    }

    pub fn millis(&mut self, key: &str, default: Duration) -> Duration {
        self.convert(key, |raw| {
            raw.parse()
                .map(Duration::from_millis)
                .map_err(|_| "ждали целое число миллисекунд".to_string())
        })
        .unwrap_or(default)
    }

    pub fn flag(&mut self, key: &str, default: bool) -> bool {
        self.convert(key, |raw| match raw.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err("ждали true или false".to_string()),
        })
        .unwrap_or(default)
    }

    pub fn list(&mut self, key: &str) -> Vec<String> {
        self.source
            .get(key)
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn secret(&mut self, key: &str) -> Option<Secret<String>> {
        let found = self.source.get(key).map(|raw| Secret::new(raw.to_string()));
        if found.is_none() {
            self.problems.push(Problem::Missing {
                key: key.to_string(),
            });
        }
        found
    }

    pub fn reject(&mut self, key: &str, reason: &str) {
        let value = self.source.get(key).unwrap_or_default().to_string();
        self.invalid(key, &value, reason);
    }

    pub fn reject_secret(&mut self, key: &str, reason: &str) {
        self.invalid(key, "***", reason);
    }

    pub fn distinct_addrs(&mut self, addrs: &[(&str, &str)]) {
        for (index, (key, addr)) in addrs.iter().enumerate() {
            if addr.ends_with(":0") {
                continue;
            }
            if let Some((first, _)) = addrs[..index].iter().find(|(_, other)| other == addr) {
                let reason = format!("тот же адрес, что у {first}: один порт не займут двое");
                self.reject(key, &reason);
            }
        }
    }

    pub fn finish(self) -> Result<(), ConfigError> {
        if self.problems.is_empty() {
            Ok(())
        } else {
            Err(ConfigError {
                problems: self.problems,
            })
        }
    }
}
