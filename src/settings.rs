use anyhow::{Context, Result};
use serde::Deserialize;
use std::{fs, io::ErrorKind, path::Path};

use crate::i18n::Locale;

pub const SETTINGS_FILE: &str = ".rustlings-settings.toml";
pub const LAYOUT_VERSION: u8 = 2;

pub struct Settings {
    pub locale: Locale,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    locale: Option<String>,
}

impl Settings {
    pub fn read() -> Result<Option<Self>> {
        let content = match fs::read_to_string(SETTINGS_FILE) {
            Ok(content) => content,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("Failed to read {SETTINGS_FILE}")),
        };
        Ok(Some(Self::parse(&content)?))
    }

    pub fn parse(content: &str) -> Result<Self> {
        let raw: Raw = toml::de::from_str(content)
            .with_context(|| format!("Failed to parse {SETTINGS_FILE}"))?;
        Ok(Self {
            locale: raw
                .locale
                .as_deref()
                .and_then(Locale::parse)
                .unwrap_or(Locale::En),
        })
    }

    pub fn render(&self) -> String {
        format!(
            "# Rustlings settings. Change the language with `rustlings lang en|ru`.\nlayout_version = {LAYOUT_VERSION}\nlocale = \"{}\"\n",
            self.locale.as_str(),
        )
    }

    pub fn write(&self) -> Result<()> {
        self.write_to(Path::new("."))
    }

    pub fn write_to(&self, root: &Path) -> Result<()> {
        fs::write(root.join(SETTINGS_FILE), self.render())
            .with_context(|| format!("Failed to write {SETTINGS_FILE}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let settings = Settings { locale: Locale::Ru };
        let parsed = Settings::parse(&settings.render()).unwrap();
        assert_eq!(parsed.locale, Locale::Ru);
        assert_eq!(Settings::parse("").unwrap().locale, Locale::En);
    }
}
