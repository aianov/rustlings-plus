use std::time::Duration;

use service_runtime::{Fields, Secret};

pub const DB_URL_ENV: &str = "DATABASE_URL";

pub const PG_URL_ENV: &str = "RUSTLINGS_PG_URL";

pub const DEFAULT_DB_URL: &str = "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbConfig {
    pub url: Secret<String>,
    pub migrate_url: Option<Secret<String>>,
    pub schema: String,
    pub max_connections: u32,
    pub acquire_timeout: Duration,
    pub statement_timeout: Duration,
}

pub fn good_schema(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && name.len() <= 63
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl DbConfig {
    pub fn new(url: &str, schema: &str) -> DbConfig {
        DbConfig {
            url: Secret::new(url.to_string()),
            migrate_url: None,
            schema: schema.to_string(),
            max_connections: 5,
            acquire_timeout: Duration::from_secs(5),
            statement_timeout: Duration::from_secs(2),
        }
    }

    pub fn policy(&self) -> crate::tx::TxPolicy {
        crate::tx::TxPolicy {
            attempts: 3,
            statement_timeout: self.statement_timeout,
        }
    }

    pub fn pool(self, max_connections: u32) -> DbConfig {
        DbConfig {
            max_connections: max_connections.max(1),
            ..self
        }
    }

    pub fn read(fields: &mut Fields<'_>, prefix: &str, schema: &str) -> Option<DbConfig> {
        let url = fields
            .secret_optional(&format!("{prefix}_DATABASE_URL"))
            .or_else(|| fields.secret_optional(DB_URL_ENV))?;
        let schema = fields.text(&format!("{prefix}_DB_SCHEMA"), schema);
        if !good_schema(&schema) {
            fields.reject(
                &format!("{prefix}_DB_SCHEMA"),
                "схема это имя: a-z, цифры и _, не больше 63 символов",
            );
        }
        let max_connections = fields.parse(&format!("{prefix}_DB_POOL"), 5u32);
        let acquire = fields.parse(&format!("{prefix}_DB_ACQUIRE_MS"), 5_000u64);
        let statement = fields.parse(&format!("{prefix}_DB_STATEMENT_MS"), 2_000u64);
        let migrate_url = fields.secret_optional(&format!("{prefix}_MIGRATE_URL"));
        Some(DbConfig {
            url,
            migrate_url,
            schema,
            max_connections,
            acquire_timeout: Duration::from_millis(acquire),
            statement_timeout: Duration::from_millis(statement),
        })
    }
}
