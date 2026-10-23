use service_runtime::{Fields, Secret};

pub const DB_URL_ENV: &str = "DATABASE_URL";

pub const PG_URL_ENV: &str = "RUSTLINGS_PG_URL";

pub const DEFAULT_DB_URL: &str = "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbConfig {
    pub url: Secret<String>,
    pub schema: String,
    pub max_connections: u32,
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
            schema: schema.to_string(),
            max_connections: 5,
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
        Some(DbConfig {
            url,
            schema,
            max_connections,
        })
    }
}
