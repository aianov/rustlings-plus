use std::time::Duration;

use service_runtime::Fields;

pub const SCYLLA_URI_ENV: &str = "RUSTLINGS_SCYLLA_URI";

pub const DEFAULT_SCYLLA_URI: &str = "127.0.0.1:59042";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    pub uri: String,
    pub keyspace: String,
    pub connect_timeout: Duration,
}

pub fn good_keyspace(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && name.len() <= 48
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl StoreConfig {
    pub fn new(uri: &str, keyspace: &str) -> StoreConfig {
        StoreConfig {
            uri: uri.to_string(),
            keyspace: keyspace.to_string(),
            connect_timeout: Duration::from_secs(3),
        }
    }

    pub fn from_env(keyspace: &str) -> StoreConfig {
        let uri = std::env::var(SCYLLA_URI_ENV)
            .ok()
            .filter(|uri| !uri.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SCYLLA_URI.to_string());
        StoreConfig::new(&uri, keyspace)
    }

    pub fn read(fields: &mut Fields<'_>, prefix: &str, keyspace: &str) -> Option<StoreConfig> {
        let uri = fields
            .optional::<String>(&format!("{prefix}_SCYLLA_URI"))
            .or_else(|| fields.optional::<String>(SCYLLA_URI_ENV))?;
        let keyspace = fields.text(&format!("{prefix}_SCYLLA_KEYSPACE"), keyspace);
        if !good_keyspace(&keyspace) {
            fields.reject(
                &format!("{prefix}_SCYLLA_KEYSPACE"),
                "keyspace это имя: a-z, цифры и _, не длиннее 48",
            );
        }
        Some(StoreConfig::new(&uri, &keyspace))
    }
}
