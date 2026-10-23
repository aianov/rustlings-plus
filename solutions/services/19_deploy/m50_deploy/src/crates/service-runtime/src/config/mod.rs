mod dotenv;
mod fields;
mod secret;

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

pub use dotenv::parse_dotenv;
pub use fields::Fields;
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

pub const FILE_SUFFIX: &str = "_FILE";

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
        Source::from_dotenv(dotenv.as_ref())?
            .over(Source::process())
            .resolve_files()
    }

    pub fn resolve_files(mut self) -> Result<Source, ConfigError> {
        let mut problems = Vec::new();
        let pointers: Vec<(String, String)> = self
            .vars
            .iter()
            .filter_map(|(key, path)| {
                let base = key.strip_suffix(FILE_SUFFIX)?;
                (!base.is_empty()).then(|| (base.to_string(), path.trim().to_string()))
            })
            .collect();
        for (key, path) in pointers {
            if self.get(&key).is_some() {
                problems.push(Problem::Invalid {
                    key: format!("{key}{FILE_SUFFIX}"),
                    value: path,
                    reason: format!("задан и {key}, и {key}{FILE_SUFFIX}: выбери одно"),
                });
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(text) => {
                    self.vars
                        .insert(key, text.trim_end_matches(['\n', '\r']).to_string());
                }
                Err(error) => problems.push(Problem::Unreadable {
                    path,
                    reason: error.to_string(),
                }),
            }
        }
        if problems.is_empty() {
            Ok(self)
        } else {
            Err(ConfigError { problems })
        }
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
        Fields::new(self)
    }
}
