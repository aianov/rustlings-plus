use std::{collections::HashMap, fmt, io, ops::RangeInclusive, time::Duration};

#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    pub signing_key: String,
    pub max_attempts: u32,
    pub backoff: Duration,
    pub max_backoff: Duration,
    pub attempt_timeout: Duration,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("signing_key", &"***")
            .field("max_attempts", &self.max_attempts)
            .field("backoff", &self.backoff)
            .field("max_backoff", &self.max_backoff)
            .field("attempt_timeout", &self.attempt_timeout)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Missing(String),
    Ambiguous(String),
    Unreadable(String),
    Empty(String),
    Invalid { name: String, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(f, "не задан ни {name}, ни {name}_FILE"),
            ConfigError::Ambiguous(name) => write!(f, "заданы сразу {name} и {name}_FILE"),
            ConfigError::Unreadable(name) => write!(f, "не читается файл из {name}_FILE"),
            ConfigError::Empty(name) => write!(f, "секрет {name} пустой"),
            ConfigError::Invalid { name, value } => write!(f, "{name}: недопустимое значение {value:?}"),
        }
    }
}

impl std::error::Error for ConfigError {}

const KEY: &str = "HOOKD_SIGNING_KEY";

impl Config {
    pub fn from_env(
        vars: &HashMap<String, String>,
        read_file: impl Fn(&str) -> io::Result<String>,
    ) -> Result<Config, ConfigError> {
        let signing_key = secret(vars, KEY, &read_file)?;
        let max_attempts = number(vars, "HOOKD_MAX_ATTEMPTS", 5, 1..=20)?;
        let backoff = number(vars, "HOOKD_BACKOFF_MS", 200, 1..=60_000)?;
        let max_backoff = number(vars, "HOOKD_MAX_BACKOFF_MS", 10_000, 1..=600_000)?;
        if max_backoff < backoff {
            return Err(ConfigError::Invalid {
                name: "HOOKD_MAX_BACKOFF_MS".to_owned(),
                value: max_backoff.to_string(),
            });
        }
        let attempt_timeout = number(vars, "HOOKD_ATTEMPT_TIMEOUT_MS", 2_000, 1..=60_000)?;
        Ok(Config {
            signing_key,
            max_attempts: u32::try_from(max_attempts).unwrap_or(u32::MAX),
            backoff: Duration::from_millis(backoff),
            max_backoff: Duration::from_millis(max_backoff),
            attempt_timeout: Duration::from_millis(attempt_timeout),
        })
    }
}

fn secret(
    vars: &HashMap<String, String>,
    name: &str,
    read_file: &impl Fn(&str) -> io::Result<String>,
) -> Result<String, ConfigError> {
    let value = match (vars.get(name), vars.get(&format!("{name}_FILE"))) {
        (Some(_), Some(_)) => return Err(ConfigError::Ambiguous(name.to_owned())),
        (None, None) => return Err(ConfigError::Missing(name.to_owned())),
        (Some(direct), None) => direct.clone(),
        (None, Some(path)) => {
            let text = read_file(path).map_err(|_| ConfigError::Unreadable(name.to_owned()))?;
            let text = text.strip_suffix('\n').unwrap_or(&text);
            text.strip_suffix('\r').unwrap_or(text).to_owned()
        }
    };
    if value.is_empty() {
        return Err(ConfigError::Empty(name.to_owned()));
    }
    Ok(value)
}

fn number(
    vars: &HashMap<String, String>,
    name: &str,
    default: u64,
    range: RangeInclusive<u64>,
) -> Result<u64, ConfigError> {
    let Some(raw) = vars.get(name) else {
        return Ok(default);
    };
    raw.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| range.contains(n))
        .ok_or_else(|| ConfigError::Invalid {
            name: name.to_owned(),
            value: raw.clone(),
        })
}
