
use std::collections::HashMap;
use std::io;

fn main() {}

pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret(***)")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    Missing(String),
    Ambiguous(String),
    Unreadable { name: String, path: String },
    Empty(String),
}

pub fn read_secret(env: &HashMap<String, String>, name: &str, read_file: &dyn Fn(&str) -> io::Result<String>) -> Result<Secret, ConfigError> {
    let direct = env.get(name);
    let path = env.get(&format!("{name}_FILE"));
    let value = match (direct, path) {
        (Some(_), Some(_)) => return Err(ConfigError::Ambiguous(name.to_string())),
        (None, None) => return Err(ConfigError::Missing(name.to_string())),
        (Some(v), None) => v.clone(),
        (None, Some(path)) => {
            let text = read_file(path).map_err(|_| ConfigError::Unreadable { name: name.to_string(), path: path.clone() })?;
            let text = text.strip_suffix('\n').unwrap_or(&text);
            text.strip_suffix('\r').unwrap_or(text).to_string()
        }
    };
    if value.is_empty() {
        return Err(ConfigError::Empty(name.to_string()));
    }
    Ok(Secret(value))
}

#[derive(Debug)]
pub struct Config {
    pub db_password: Secret,
    pub jwt_secret: Secret,
    pub nats_token: Option<Secret>,
}

impl Config {
    /// `DATABASE_PASSWORD` и `JWT_SECRET` обязательны, `NATS_TOKEN` нет (но если он есть, то должен быть корректным).
    pub fn load(env: &HashMap<String, String>, read_file: &dyn Fn(&str) -> io::Result<String>) -> Result<Config, ConfigError> {
        let nats_token = match read_secret(env, "NATS_TOKEN", read_file) {
            Ok(secret) => Some(secret),
            Err(ConfigError::Missing(_)) => None,
            Err(other) => return Err(other),
        };
        Ok(Config { db_password: read_secret(env, "DATABASE_PASSWORD", read_file)?, jwt_secret: read_secret(env, "JWT_SECRET", read_file)?, nats_token })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn files(content: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> io::Result<String> {
        move |path| content.iter().find(|(p, _)| *p == path).map(|(_, c)| c.to_string()).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "нет файла"))
    }

    #[test]
    fn a_plain_variable_works() {
        let s = read_secret(&env(&[("DB", "hunter2")]), "DB", &files(&[])).unwrap();
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn a_file_works_and_only_one_trailing_newline_is_removed() {
        let read = files(&[("/run/secrets/db", "pa ss\n"), ("/run/secrets/crlf", "pw\r\n"), ("/run/secrets/two", "pw\n\n"), ("/run/secrets/space", "pw \n")]);
        assert_eq!(read_secret(&env(&[("DB_FILE", "/run/secrets/db")]), "DB", &read).unwrap().expose(), "pa ss");
        assert_eq!(read_secret(&env(&[("DB_FILE", "/run/secrets/crlf")]), "DB", &read).unwrap().expose(), "pw");
        assert_eq!(read_secret(&env(&[("DB_FILE", "/run/secrets/two")]), "DB", &read).unwrap().expose(), "pw\n", "удаляется один перевод строки, остальное часть пароля");
        assert_eq!(read_secret(&env(&[("DB_FILE", "/run/secrets/space")]), "DB", &read).unwrap().expose(), "pw ", "пробел на конце тоже часть пароля");
    }

    #[test]
    fn both_neither_empty_and_unreadable_are_errors() {
        let read = files(&[("/f", "x\n"), ("/empty", "\n")]);
        let e = |pairs| read_secret(&env(pairs), "DB", &read).unwrap_err();
        assert_eq!(e(&[("DB", "a"), ("DB_FILE", "/f")]), ConfigError::Ambiguous("DB".into()));
        assert_eq!(e(&[]), ConfigError::Missing("DB".into()));
        assert_eq!(e(&[("DB", "")]), ConfigError::Empty("DB".into()));
        assert_eq!(e(&[("DB_FILE", "/empty")]), ConfigError::Empty("DB".into()));
        assert_eq!(e(&[("DB_FILE", "/nope")]), ConfigError::Unreadable { name: "DB".into(), path: "/nope".into() });
    }

    #[test]
    fn secrets_never_appear_in_debug_output() {
        let read = files(&[("/run/secrets/jwt", "very-secret-jwt-key\n")]);
        let cfg = Config::load(&env(&[("DATABASE_PASSWORD", "very-secret-db-pass"), ("JWT_SECRET_FILE", "/run/secrets/jwt"), ("NATS_TOKEN", "very-secret-token")]), &read).unwrap();
        let shown = format!("{cfg:?}");
        for secret in ["very-secret-db-pass", "very-secret-jwt-key", "very-secret-token"] {
            assert!(!shown.contains(secret), "секрет {secret:?} попал в Debug: {shown}");
        }
        assert!(shown.contains("***"));
        assert_eq!(cfg.jwt_secret.expose(), "very-secret-jwt-key");
    }

    #[test]
    fn the_optional_token_may_be_absent_but_not_broken() {
        let read = files(&[]);
        let ok = Config::load(&env(&[("DATABASE_PASSWORD", "a"), ("JWT_SECRET", "b")]), &read).unwrap();
        assert!(ok.nats_token.is_none());
        let broken = Config::load(&env(&[("DATABASE_PASSWORD", "a"), ("JWT_SECRET", "b"), ("NATS_TOKEN_FILE", "/нет")]), &read);
        assert!(matches!(broken, Err(ConfigError::Unreadable { .. })), "заданный, но нечитаемый токен это ошибка, а не молчаливое отсутствие");
        assert_eq!(Config::load(&env(&[("JWT_SECRET", "b")]), &read).unwrap_err(), ConfigError::Missing("DATABASE_PASSWORD".into()));
    }
}
