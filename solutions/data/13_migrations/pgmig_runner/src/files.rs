use std::path::Path;

use crate::{MigrateError, Migration};

pub fn parse_dir(dir: &Path) -> Result<Vec<Migration>, MigrateError> {
    let mut found: Vec<Migration> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let bad = || MigrateError::BadFileName(file_name.clone());
        let (digits, name) = stem.split_once('_').ok_or_else(bad)?;
        if digits.is_empty() || name.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        let version: i64 = digits.parse().map_err(|_| bad())?;
        if found.iter().any(|m| m.version == version) {
            return Err(MigrateError::DuplicateVersion(version));
        }
        found.push(Migration { version, name: name.to_string(), sql: std::fs::read_to_string(&path)? });
    }
    found.sort_by_key(|m| m.version);
    Ok(found)
}
