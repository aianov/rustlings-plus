use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

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

pub fn parse_or<T: FromStr>(name: &str, raw: Option<String>, default: T) -> Result<T, ConfigError> {
    match raw {
        None => Ok(default),
        Some(value) => value
            .trim()
            .parse()
            .map_err(|_| ConfigError::Invalid { name: name.to_string(), value }),
    }
}

pub fn required(name: &str, raw: Option<String>) -> Result<String, ConfigError> {
    match raw {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::Missing(name.to_string())),
    }
}

#[derive(Debug, PartialEq)]
pub struct DotenvError {
    pub line: usize,
}

pub fn parse_dotenv(text: &str) -> Result<HashMap<String, String>, DotenvError> {
    let mut out = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            return Err(DotenvError { line: index + 1 });
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(DotenvError { line: index + 1 });
        }
        out.insert(key.to_string(), unquote(value.trim()));
    }
    Ok(out)
}

fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(quote).and_then(|v| v.strip_suffix(quote)) {
            return inner.to_string();
        }
    }
    match value.find(" #") {
        Some(at) => value[..at].trim_end().to_string(),
        None => value.to_string(),
    }
}

pub fn layered<'a>(
    file: &'a HashMap<String, String>,
    real: impl Fn(&str) -> Option<String> + 'a,
) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| real(name).or_else(|| file.get(name).cloned())
}

pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Secret(value)
    }

    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}
