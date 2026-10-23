use serde::{Deserialize, Serialize};

pub const THEMES: [&str; 2] = ["light", "dark"];

pub const LANGUAGES: [&str; 2] = ["ru", "en"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<bool>,
}

impl Settings {
    pub fn problem(&self) -> Option<String> {
        if let Some(theme) = self
            .theme
            .as_deref()
            .filter(|theme| !THEMES.contains(theme))
        {
            return Some(format!("тема «{theme}» не из {}", THEMES.join(", ")));
        }
        if let Some(language) = self
            .language
            .as_deref()
            .filter(|language| !LANGUAGES.contains(language))
        {
            return Some(format!("язык «{language}» не из {}", LANGUAGES.join(", ")));
        }
        None
    }
}
