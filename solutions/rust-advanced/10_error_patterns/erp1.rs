use std::collections::HashMap;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Missing { key: String },
    NotANumber { key: String, value: String },
    OutOfRange { key: String, value: u32, min: u32, max: u32 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing { key } => write!(f, "missing setting {key:?}"),
            ConfigError::NotANumber { key, value } => write!(f, "setting {key:?} is not a number: {value:?}"),
            ConfigError::OutOfRange { key, value, min, max } => {
                write!(f, "setting {key:?} is {value}, expected {min}..={max}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn read_port(settings: &HashMap<String, String>) -> Result<u16, ConfigError> {
    let key = "port";
    let raw = settings.get(key).ok_or_else(|| ConfigError::Missing { key: key.to_string() })?;
    let value: u32 = raw
        .parse()
        .map_err(|_| ConfigError::NotANumber { key: key.to_string(), value: raw.clone() })?;
    if !(1024..=65535).contains(&value) {
        return Err(ConfigError::OutOfRange { key: key.to_string(), value, min: 1024, max: 65535 });
    }
    Ok(value as u16)
}

fn main() {
    let settings = HashMap::from([("port".to_string(), "8080".to_string())]);
    println!("{:?}", read_port(&settings));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn with_port(value: &str) -> HashMap<String, String> {
        HashMap::from([("port".to_string(), value.to_string())])
    }

    #[test]
    fn good_port() {
        assert_eq!(read_port(&with_port("8080")), Ok(8080));
        assert_eq!(read_port(&with_port("1024")), Ok(1024), "нижняя граница входит");
        assert_eq!(read_port(&with_port("65535")), Ok(65535), "верхняя граница входит");
    }

    #[test]
    fn each_failure_has_its_own_variant() {
        assert_eq!(read_port(&HashMap::new()), Err(ConfigError::Missing { key: "port".into() }));
        assert_eq!(
            read_port(&with_port("80a")),
            Err(ConfigError::NotANumber { key: "port".into(), value: "80a".into() }),
            "буквы в числе: это NotANumber, а не выход за границы"
        );
        assert_eq!(
            read_port(&with_port("70000")),
            Err(ConfigError::OutOfRange { key: "port".into(), value: 70000, min: 1024, max: 65535 }),
            "число в u32 помещается, но в порт нет: это OutOfRange"
        );
        assert!(matches!(read_port(&with_port("80")), Err(ConfigError::OutOfRange { value: 80, .. })));
        assert!(matches!(read_port(&with_port("-1")), Err(ConfigError::NotANumber { .. })), "минус не разбирается как u32");
    }

    #[test]
    fn messages_are_for_humans() {
        let missing = read_port(&HashMap::new()).unwrap_err();
        assert_eq!(missing.to_string(), "missing setting \"port\"");
        let bad = read_port(&with_port("80a")).unwrap_err();
        assert_eq!(bad.to_string(), "setting \"port\" is not a number: \"80a\"");
        let range = read_port(&with_port("80")).unwrap_err();
        assert_eq!(range.to_string(), "setting \"port\" is 80, expected 1024..=65535");
    }

    #[test]
    fn works_as_a_std_error() {
        let boxed: Box<dyn Error> = Box::new(read_port(&HashMap::new()).unwrap_err());
        assert_eq!(boxed.to_string(), "missing setting \"port\"");
        assert!(boxed.source().is_none(), "у этой ошибки нет причины");
    }
}
