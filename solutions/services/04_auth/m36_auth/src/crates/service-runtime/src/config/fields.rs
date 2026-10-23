use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use super::{ConfigError, Problem, Secret, Source};

pub struct Fields<'a> {
    source: &'a Source,
    problems: Vec<Problem>,
}

impl<'a> Fields<'a> {
    pub(super) fn new(source: &'a Source) -> Fields<'a> {
        Fields {
            source,
            problems: Vec::new(),
        }
    }

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

    pub fn secret_optional(&mut self, key: &str) -> Option<Secret<String>> {
        self.source.get(key).map(|raw| Secret::new(raw.to_string()))
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
